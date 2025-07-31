use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use uuid::Uuid;
use tracing::{info, warn};

use crate::ast::treesitter::parsers::get_ast_parser_by_filename;
use crate::ast::treesitter::ast_instance_structs::AstSymbolInstanceArc;
use crate::ast::treesitter::structs::SymbolType;
use crate::codegraph::types::{FunctionInfo, CallRelation, ParameterInfo};
use crate::codegraph::CodeGraph;

/// 代码解析器，负责解析源代码文件并提取函数调用关系
pub struct CodeParser {
    /// 文件路径 -> AST符号映射
    file_asts: HashMap<PathBuf, Vec<AstSymbolInstanceArc>>,
    /// 函数名 -> 函数信息映射（用于解析调用关系）
    function_registry: HashMap<String, FunctionInfo>,
}

impl CodeParser {
    pub fn new() -> Self {
        Self {
            file_asts: HashMap::new(),
            function_registry: HashMap::new(),
        }
    }

    /// 扫描目录下的所有支持的文件
    pub fn scan_directory(&mut self, dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        self._scan_directory_recursive(dir, &mut files);
        files
    }

    fn _scan_directory_recursive(&self, dir: &Path, files: &mut Vec<PathBuf>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // 跳过常见的忽略目录
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with('.') || name == "target" || name == "node_modules" || name == "__pycache__" {
                            continue;
                        }
                    }
                    self._scan_directory_recursive(&path, files);
                } else if self.is_supported_file(&path) {
                    files.push(path);
                }
            }
        }
    }

    /// 判断文件是否为支持的源代码文件
    fn is_supported_file(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            matches!(ext.to_lowercase().as_str(),
                "cpp" | "cc" | "cxx" | "c++" | "c" | "h" | "hpp" | "hxx" | "hh" |
                "inl" | "inc" | "tpp" | "tpl" |
                "py" | "py3" | "pyx" |
                "java" |
                "js" | "jsx" |
                "rs" |
                "ts" |
                "tsx"
            )
        } else {
            false
        }
    }

    /// 解析单个文件
    pub fn parse_file(&mut self, file_path: &PathBuf) -> Result<(), String> {
        info!("Parsing file: {}", file_path.display());
        
        // 获取对应的解析器
        let (mut parser, _language_id) = get_ast_parser_by_filename(file_path)
            .map_err(|e| format!("Failed to get parser for {}: {}", file_path.display(), e.message))?;

        // 读取文件内容
        let code = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;

        // 解析AST
        let ast = parser.parse(&code, file_path);
        self.file_asts.insert(file_path.clone(), ast);

        Ok(())
    }

    /// 解析目录下的所有文件
    pub fn parse_directory(&mut self, dir: &Path) -> Result<(), String> {
        let files = self.scan_directory(dir);
        info!("Found {} files to parse", files.len());

        for file in files {
            if let Err(e) = self.parse_file(&file) {
                warn!("Failed to parse {}: {}", file.display(), e);
            }
        }

        Ok(())
    }

    /// 从AST中提取函数信息
    pub fn extract_functions(&mut self) -> Vec<FunctionInfo> {
        let mut functions = Vec::new();

        for (file_path, ast) in &self.file_asts {
            let mut current_function_stack: Vec<Uuid> = Vec::new();

            for symbol in ast {
                let symbol_guard = symbol.read();
                
                match symbol_guard.symbol_type() {
                    SymbolType::FunctionDeclaration => {
                        let function_info = self._create_function_info(symbol_guard.as_ref(), file_path);
                        let function_id = function_info.id;
                        
                        // 注册函数
                        self.function_registry.insert(function_info.name.clone(), function_info.clone());
                        functions.push(function_info);
                        
                        // 更新调用栈
                        current_function_stack.push(function_id);
                    }
                    SymbolType::FunctionCall => {
                        if let Some(caller_id) = current_function_stack.last() {
                            // 尝试解析被调用的函数
                            let callee_name = symbol_guard.name();
                            if let Some(callee_info) = self.function_registry.get(callee_name) {
                                // 找到匹配的函数，创建调用关系
                                let caller_info = functions.iter().find(|f| f.id == *caller_id).unwrap();
                                
                                // 这里可以创建CallRelation，但为了简化，我们先收集函数信息
                                info!("Found call: {} -> {} in {}", 
                                      caller_info.name, callee_info.name, file_path.display());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        functions
    }

    /// 创建函数信息
    fn _create_function_info(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance, file_path: &PathBuf) -> FunctionInfo {
        FunctionInfo {
            id: *symbol.guid(),
            name: symbol.name().to_string(),
            file_path: file_path.clone(),
            line_start: symbol.full_range().start_point.row + 1,
            line_end: symbol.full_range().end_point.row + 1,
            namespace: symbol.namespace().to_string(),
            language: symbol.language().to_string(),
            signature: self._extract_function_signature(symbol),
            return_type: self._extract_return_type(symbol),
            parameters: self._extract_parameters(symbol),
        }
    }

    /// 提取函数签名
    fn _extract_function_signature(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> Option<String> {
        use crate::ast::treesitter::ast_instance_structs::FunctionDeclaration;
        use crate::ast::treesitter::language_id::LanguageId;
        
        // 尝试将symbol转换为FunctionDeclaration
        if let Some(func_decl) = symbol.as_any().downcast_ref::<FunctionDeclaration>() {
            let mut signature = String::new();
            
            // 根据语言类型构建不同的签名格式
            match symbol.language() {
                LanguageId::Rust => {
                    // Rust: fn name<T>(param1: Type1, param2: Type2) -> ReturnType
                    signature.push_str("fn ");
                    signature.push_str(symbol.name());
                    
                    // 添加模板参数
                    if !func_decl.template_types.is_empty() {
                        signature.push('<');
                        let template_names: Vec<String> = func_decl.template_types.iter()
                            .map(|t| t.to_string())
                            .collect();
                        signature.push_str(&template_names.join(", "));
                        signature.push('>');
                    }
                    
                    // 添加参数
                    signature.push('(');
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            if !arg.name.is_empty() {
                                param.push_str(&arg.name);
                                param.push_str(": ");
                            }
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(&type_def.to_string());
                            } else {
                                param.push_str("_");
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                    
                    // 添加返回类型
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(" -> ");
                        signature.push_str(&return_type.to_string());
                    }
                }
                
                LanguageId::Cpp | LanguageId::C => {
                    // C/C++: ReturnType name<T>(Type1 param1, Type2 param2)
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(&return_type.to_string());
                        signature.push(' ');
                    } else {
                        signature.push_str("void ");
                    }
                    
                    signature.push_str(symbol.name());
                    
                    // 添加模板参数
                    if !func_decl.template_types.is_empty() {
                        signature.push('<');
                        let template_names: Vec<String> = func_decl.template_types.iter()
                            .map(|t| t.to_string())
                            .collect();
                        signature.push_str(&template_names.join(", "));
                        signature.push('>');
                    }
                    
                    // 添加参数
                    signature.push('(');
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(&type_def.to_string());
                                if !arg.name.is_empty() {
                                    param.push(' ');
                                    param.push_str(&arg.name);
                                }
                            } else {
                                param.push_str("void");
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                }
                
                LanguageId::Java => {
                    // Java: ReturnType name(Type1 param1, Type2 param2)
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(&return_type.to_string());
                        signature.push(' ');
                    } else {
                        signature.push_str("void ");
                    }
                    
                    signature.push_str(symbol.name());
                    signature.push('(');
                    
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(&type_def.to_string());
                                if !arg.name.is_empty() {
                                    param.push(' ');
                                    param.push_str(&arg.name);
                                }
                            } else {
                                param.push_str("Object");
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                }
                
                LanguageId::Python => {
                    // Python: def name(param1: Type1, param2: Type2) -> ReturnType:
                    signature.push_str("def ");
                    signature.push_str(symbol.name());
                    signature.push('(');
                    
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            param.push_str(&arg.name);
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(": ");
                                param.push_str(&type_def.to_string());
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                    
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(" -> ");
                        signature.push_str(&return_type.to_string());
                    }
                    signature.push(':');
                }
                
                LanguageId::JavaScript | LanguageId::TypeScript | LanguageId::TypeScriptReact => {
                    // JavaScript/TypeScript: function name(param1: Type1, param2: Type2): ReturnType
                    signature.push_str("function ");
                    signature.push_str(symbol.name());
                    signature.push('(');
                    
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            param.push_str(&arg.name);
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(": ");
                                param.push_str(&type_def.to_string());
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                    
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(": ");
                        signature.push_str(&return_type.to_string());
                    }
                }
                
                LanguageId::Go => {
                    // Go: func name(param1 Type1, param2 Type2) ReturnType
                    signature.push_str("func ");
                    signature.push_str(symbol.name());
                    signature.push('(');
                    
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            param.push_str(&arg.name);
                            if let Some(ref type_def) = arg.type_ {
                                param.push(' ');
                                param.push_str(&type_def.to_string());
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                    
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push(' ');
                        signature.push_str(&return_type.to_string());
                    }
                }
                
                _ => {
                    // 通用格式：name(param1: Type1, param2: Type2) -> ReturnType
                    signature.push_str(symbol.name());
                    signature.push('(');
                    
                    let param_signatures: Vec<String> = func_decl.args.iter()
                        .map(|arg| {
                            let mut param = String::new();
                            param.push_str(&arg.name);
                            if let Some(ref type_def) = arg.type_ {
                                param.push_str(": ");
                                param.push_str(&type_def.to_string());
                            }
                            param
                        })
                        .collect();
                    signature.push_str(&param_signatures.join(", "));
                    signature.push(')');
                    
                    if let Some(ref return_type) = func_decl.return_type {
                        signature.push_str(" -> ");
                        signature.push_str(&return_type.to_string());
                    }
                }
            }
            
            Some(signature)
        } else {
            None
        }
    }

    /// 提取返回类型
    fn _extract_return_type(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> Option<String> {
        use crate::ast::treesitter::ast_instance_structs::FunctionDeclaration;
        
        // 尝试将symbol转换为FunctionDeclaration
        if let Some(func_decl) = symbol.as_any().downcast_ref::<FunctionDeclaration>() {
            func_decl.return_type.as_ref().map(|t| t.to_string())
        } else {
            None
        }
    }

    /// 提取参数信息
    fn _extract_parameters(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> Vec<ParameterInfo> {
        use crate::ast::treesitter::ast_instance_structs::FunctionDeclaration;
        
        // 尝试将symbol转换为FunctionDeclaration
        if let Some(func_decl) = symbol.as_any().downcast_ref::<FunctionDeclaration>() {
            func_decl.args.iter()
                .map(|arg| ParameterInfo {
                    name: arg.name.clone(),
                    type_name: arg.type_.as_ref().map(|t| t.to_string()),
                    default_value: None, // 目前AST中没有默认值信息，设为None
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// 构建完整的代码图
    pub fn build_code_graph(&mut self, dir: &Path) -> Result<CodeGraph, String> {
        // 1. 解析所有文件
        self.parse_directory(dir)?;
        
        // 2. 构建代码图
        let mut code_graph = CodeGraph::new();
        
        // 3. 提取函数信息并直接添加到代码图
        for (file_path, ast) in &self.file_asts {
            for symbol in ast {
                let symbol_guard = symbol.read();
                
                if symbol_guard.symbol_type() == SymbolType::FunctionDeclaration {
                    let function_info = self._create_function_info(symbol_guard.as_ref(), file_path);
                    code_graph.add_function(function_info);
                }
            }
        }
        
        // 4. 分析调用关系
        self._analyze_call_relations(&mut code_graph);
        
        // 5. 更新统计信息
        code_graph.update_stats();
        
        Ok(code_graph)
    }

    /// 分析调用关系
    fn _analyze_call_relations(&self, code_graph: &mut CodeGraph) {
        for (_file_path, ast) in &self.file_asts {
            let mut current_function_stack: Vec<Uuid> = Vec::new();

            for symbol in ast {
                let symbol_guard = symbol.read();
                
                match symbol_guard.symbol_type() {
                    SymbolType::FunctionDeclaration => {
                        // 找到当前函数
                        let function_name = symbol_guard.name();
                        if let Some(function_info) = code_graph.find_functions_by_name(function_name).first() {
                            current_function_stack.push(function_info.id);
                        }
                    }
                    SymbolType::FunctionCall => {
                        if let Some(caller_id) = current_function_stack.last() {
                            let callee_name = symbol_guard.name();
                            
                            // 查找被调用的函数
                            let callee_functions = code_graph.find_functions_by_name(callee_name);
                            
                            // 收集所有需要添加的关系
                            let mut relations_to_add = Vec::new();
                            
                            if let Some(caller_info) = code_graph.functions.get(caller_id) {
                                for callee_info in callee_functions {
                                    let relation = CallRelation {
                                        caller_id: *caller_id,
                                        callee_id: callee_info.id,
                                        caller_name: caller_info.name.clone(),
                                        callee_name: callee_info.name.clone(),
                                        caller_file: caller_info.file_path.clone(),
                                        callee_file: callee_info.file_path.clone(),
                                        line_number: symbol_guard.full_range().start_point.row + 1,
                                        is_resolved: true,
                                    };
                                    
                                    relations_to_add.push(relation);
                                }
                            }
                            
                            // 在循环外添加关系
                            for relation in relations_to_add {
                                code_graph.add_call_relation(relation);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

impl Default for CodeParser {
    fn default() -> Self {
        Self::new()
    }
} 