use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use uuid::Uuid;
use tracing::{info, warn};


use crate::ast::treesitter::parsers::get_ast_parser_by_filename;
use crate::ast::treesitter::ast_instance_structs::AstSymbolInstanceArc;
use crate::ast::treesitter::structs::SymbolType;
use crate::codegraph::types::{FunctionInfo, CallRelation, ParameterInfo, PetCodeGraph};
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

        
        // 首先检查是否为函数声明
        if symbol.symbol_type() != crate::ast::treesitter::structs::SymbolType::FunctionDeclaration {
            return None;
        }
        
        // 尝试将symbol转换为FunctionDeclaration
        let func_decl = match symbol.as_any().downcast_ref::<FunctionDeclaration>() {
            Some(decl) => decl,
            None => {
                // 如果无法转换为FunctionDeclaration，尝试构建基本签名
                return self._build_basic_signature(symbol);
            }
        };
        
        // 使用新的完整签名构建方法
        let signature = self._build_complete_signature(symbol, func_decl);
        Some(signature)
    }
    
    /// 构建基本签名（当无法转换为FunctionDeclaration时使用）
    fn _build_basic_signature(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> Option<String> {
        let mut signature = String::new();
        
        // 根据语言添加适当的关键字
        match symbol.language() {
            crate::ast::treesitter::language_id::LanguageId::Rust => signature.push_str("fn "),
            crate::ast::treesitter::language_id::LanguageId::Python => signature.push_str("def "),
            crate::ast::treesitter::language_id::LanguageId::JavaScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => signature.push_str("function "),
            crate::ast::treesitter::language_id::LanguageId::Go => signature.push_str("func "),
            _ => {}
        }
        
        // 函数名
        signature.push_str(symbol.name());
        signature.push_str("(...)");
        
        Some(signature)
    }

    /// 安全地获取类型名称
    fn _safe_type_name(&self, type_def: &crate::ast::treesitter::ast_instance_structs::TypeDef) -> String {
        if let Some(name) = &type_def.name {
            name.clone()
        } else if let Some(inference_info) = &type_def.inference_info {
            inference_info.clone()
        } else {
            "unknown".to_string()
        }
    }

    /// 安全地构建参数签名
    fn _build_param_signature(&self, arg: &crate::ast::treesitter::ast_instance_structs::FunctionArg, 
                             language: &crate::ast::treesitter::language_id::LanguageId) -> String {
        let mut param = String::new();
        
        match language {
            crate::ast::treesitter::language_id::LanguageId::Rust => {
                // Rust: name: Type
                if !arg.name.is_empty() {
                    param.push_str(&arg.name);
                    param.push_str(": ");
                }
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(&self._safe_type_name(type_def));
                } else {
                    param.push_str("_");
                }
            }
            
            crate::ast::treesitter::language_id::LanguageId::Cpp | 
            crate::ast::treesitter::language_id::LanguageId::C => {
                // C/C++: Type name
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(&self._safe_type_name(type_def));
                    if !arg.name.is_empty() {
                        param.push(' ');
                        param.push_str(&arg.name);
                    }
                } else {
                    param.push_str("void");
                }
            }
            
            crate::ast::treesitter::language_id::LanguageId::Java => {
                // Java: Type name
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(&self._safe_type_name(type_def));
                    if !arg.name.is_empty() {
                        param.push(' ');
                        param.push_str(&arg.name);
                    }
                } else {
                    param.push_str("Object");
                }
            }
            
            crate::ast::treesitter::language_id::LanguageId::Python => {
                // Python: name: Type
                param.push_str(&arg.name);
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(": ");
                    param.push_str(&self._safe_type_name(type_def));
                }
            }
            
            crate::ast::treesitter::language_id::LanguageId::JavaScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => {
                // JavaScript/TypeScript: name: Type
                param.push_str(&arg.name);
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(": ");
                    param.push_str(&self._safe_type_name(type_def));
                }
            }
            
            crate::ast::treesitter::language_id::LanguageId::Go => {
                // Go: name Type
                param.push_str(&arg.name);
                if let Some(ref type_def) = arg.type_ {
                    param.push(' ');
                    param.push_str(&self._safe_type_name(type_def));
                }
            }
            
            _ => {
                // 通用格式：name: Type
                param.push_str(&arg.name);
                if let Some(ref type_def) = arg.type_ {
                    param.push_str(": ");
                    param.push_str(&self._safe_type_name(type_def));
                }
            }
        }
        
        param
    }

    /// 构建模板参数字符串
    fn _build_template_params(&self, template_types: &[crate::ast::treesitter::ast_instance_structs::TypeDef]) -> String {
        if template_types.is_empty() {
            return String::new();
        }
        
        let template_names: Vec<String> = template_types.iter()
            .filter_map(|t| t.name.as_ref())
            .map(|name| name.to_string())
            .collect();
        
        if template_names.is_empty() {
            return String::new();
        }
        
        format!("<{}>", template_names.join(", "))
    }

    /// 构建返回类型字符串
    fn _build_return_type(&self, return_type: &Option<crate::ast::treesitter::ast_instance_structs::TypeDef>,
                         language: &crate::ast::treesitter::language_id::LanguageId) -> String {
        if let Some(ref type_def) = return_type {
            match language {
                crate::ast::treesitter::language_id::LanguageId::Rust => {
                    format!(" -> {}", self._safe_type_name(type_def))
                }
                crate::ast::treesitter::language_id::LanguageId::Python => {
                    format!(" -> {}", self._safe_type_name(type_def))
                }
                crate::ast::treesitter::language_id::LanguageId::JavaScript | 
                crate::ast::treesitter::language_id::LanguageId::TypeScript | 
                crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => {
                    format!(": {}", self._safe_type_name(type_def))
                }
                crate::ast::treesitter::language_id::LanguageId::Go => {
                    format!(" {}", self._safe_type_name(type_def))
                }
                _ => {
                    format!(" -> {}", self._safe_type_name(type_def))
                }
            }
        } else {
            String::new()
        }
    }

    /// 验证函数声明的完整性
    fn _validate_function_declaration(&self, func_decl: &crate::ast::treesitter::ast_instance_structs::FunctionDeclaration) -> bool {
        // 检查函数名是否为空
        if func_decl.ast_fields.name.is_empty() {
            return false;
        }
        
        // 检查参数是否有重复名称
        let mut param_names = std::collections::HashSet::new();
        for arg in &func_decl.args {
            if !arg.name.is_empty() && !param_names.insert(&arg.name) {
                // 发现重复的参数名
                return false;
            }
        }
        
        true
    }

    /// 处理特殊字符和转义
    fn _escape_identifier(&self, identifier: &str) -> String {
        // 检查是否需要转义
        let needs_escaping = identifier.chars().any(|c| {
            !c.is_alphanumeric() && c != '_' && c != '-'
        });
        
        if needs_escaping {
            // 简单的转义处理，实际应用中可能需要更复杂的逻辑
            identifier.replace("\"", "\\\"").replace("'", "\\'")
        } else {
            identifier.to_string()
        }
    }

    /// 构建完整的函数签名（带错误处理）
    fn _build_complete_signature(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance,
                                func_decl: &crate::ast::treesitter::ast_instance_structs::FunctionDeclaration) -> String {
        // 验证函数声明
        if !self._validate_function_declaration(func_decl) {
            warn!("Invalid function declaration for {}: validation failed", symbol.name());
            return self._build_fallback_signature(symbol);
        }
        
        let mut signature = String::new();
        let language = symbol.language();
        
        // 添加命名空间前缀
        signature.push_str(&self._build_namespace_prefix(symbol));
        
        // 添加函数修饰符
        signature.push_str(&self._build_function_modifiers(symbol, language));
        
        // 添加函数关键字
        match language {
            crate::ast::treesitter::language_id::LanguageId::Rust => signature.push_str("fn "),
            crate::ast::treesitter::language_id::LanguageId::Python => signature.push_str("def "),
            crate::ast::treesitter::language_id::LanguageId::JavaScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => signature.push_str("function "),
            crate::ast::treesitter::language_id::LanguageId::Go => signature.push_str("func "),
            _ => {}
        }
        
        // 添加函数名
        signature.push_str(&self._escape_identifier(symbol.name()));
        
        // 添加模板参数
        let template_params = self._build_template_params(&func_decl.template_types);
        if !template_params.is_empty() {
            signature.push_str(&template_params);
        }
        
        // 添加参数列表
        signature.push('(');
        
        // 使用可变参数处理
        let param_signatures = self._build_varargs_signature(&func_decl.args, language);
        signature.push_str(&param_signatures);
        
        signature.push(')');
        
        // 添加返回类型
        signature.push_str(&self._build_return_type(&func_decl.return_type, language));
        
        // 为Python添加冒号
        if matches!(language, crate::ast::treesitter::language_id::LanguageId::Python) {
            signature.push(':');
        }
        
        signature
    }

    /// 构建后备签名（当验证失败时使用）
    fn _build_fallback_signature(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> String {
        let mut signature = String::new();
        
        // 添加函数关键字
        match symbol.language() {
            crate::ast::treesitter::language_id::LanguageId::Rust => signature.push_str("fn "),
            crate::ast::treesitter::language_id::LanguageId::Python => signature.push_str("def "),
            crate::ast::treesitter::language_id::LanguageId::JavaScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => signature.push_str("function "),
            crate::ast::treesitter::language_id::LanguageId::Go => signature.push_str("func "),
            _ => {}
        }
        
        // 添加函数名
        signature.push_str(&self._escape_identifier(symbol.name()));
        signature.push_str("(...)");
        
        signature
    }

    /// 处理嵌套类型
    fn _build_nested_type_signature(&self, type_def: &crate::ast::treesitter::ast_instance_structs::TypeDef) -> String {
        let mut signature = self._safe_type_name(type_def);
        
        if !type_def.nested_types.is_empty() {
            signature.push('<');
            let nested_signatures: Vec<String> = type_def.nested_types.iter()
                .map(|nested| self._build_nested_type_signature(nested))
                .collect();
            signature.push_str(&nested_signatures.join(", "));
            signature.push('>');
        }
        
        signature
    }

    /// 处理默认参数值
    fn _build_param_with_default(&self, arg: &crate::ast::treesitter::ast_instance_structs::FunctionArg,
                                language: &crate::ast::treesitter::language_id::LanguageId,
                                default_value: Option<&str>) -> String {
        let mut param = self._build_param_signature(arg, language);
        
        if let Some(default) = default_value {
            match language {
                crate::ast::treesitter::language_id::LanguageId::Python => {
                    param.push_str(" = ");
                    param.push_str(default);
                }
                crate::ast::treesitter::language_id::LanguageId::JavaScript | 
                crate::ast::treesitter::language_id::LanguageId::TypeScript | 
                crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => {
                    param.push_str(" = ");
                    param.push_str(default);
                }
                _ => {
                    // 其他语言可能不支持默认参数值，或者需要不同的语法
                    param.push_str(" /* default: ");
                    param.push_str(default);
                    param.push_str(" */");
                }
            }
        }
        
        param
    }

    /// 处理可变参数
    fn _build_varargs_signature(&self, args: &[crate::ast::treesitter::ast_instance_structs::FunctionArg],
                               language: &crate::ast::treesitter::language_id::LanguageId) -> String {
        let mut param_signatures: Vec<String> = args.iter()
            .map(|arg| self._build_param_signature(arg, language))
            .collect();
        
        // 根据语言添加可变参数语法
        match language {
            crate::ast::treesitter::language_id::LanguageId::Python => {
                if !param_signatures.is_empty() {
                    param_signatures.push("*args".to_string());
                    param_signatures.push("**kwargs".to_string());
                }
            }
            crate::ast::treesitter::language_id::LanguageId::JavaScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScript | 
            crate::ast::treesitter::language_id::LanguageId::TypeScriptReact => {
                if !param_signatures.is_empty() {
                    param_signatures.push("...args".to_string());
                }
            }
            crate::ast::treesitter::language_id::LanguageId::Rust => {
                if !param_signatures.is_empty() {
                    param_signatures.push("...".to_string());
                }
            }
            _ => {}
        }
        
        param_signatures.join(", ")
    }

    /// 处理函数修饰符
    fn _build_function_modifiers(&self, _symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance,
                                language: &crate::ast::treesitter::language_id::LanguageId) -> String {
        let mut modifiers = String::new();
        
        // 这里可以根据AST中的其他信息添加修饰符
        // 比如 public, private, static, async 等
        // 目前这是一个占位符实现
        
        match language {
            crate::ast::treesitter::language_id::LanguageId::Java => {
                // Java 可能有 public, private, static 等修饰符
                modifiers.push_str("public ");
            }
            crate::ast::treesitter::language_id::LanguageId::Cpp => {
                // C++ 可能有 virtual, static, const 等修饰符
                modifiers.push_str("virtual ");
            }
            _ => {}
        }
        
        modifiers
    }

    /// 处理命名空间
    fn _build_namespace_prefix(&self, symbol: &dyn crate::ast::treesitter::ast_instance_structs::AstSymbolInstance) -> String {
        let namespace = symbol.namespace();
        if !namespace.is_empty() && namespace != "global" {
            return format!("{}::", namespace);
        }
        String::new()
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

    /// 构建基于petgraph的代码图
    pub fn build_petgraph_code_graph(&mut self, dir: &Path) -> Result<PetCodeGraph, String> {
        // 1. 解析所有文件
        self.parse_directory(dir)?;
        
        // 2. 构建petgraph代码图
        let mut code_graph = PetCodeGraph::new();
        
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
        self._analyze_petgraph_call_relations(&mut code_graph);
        
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

    /// 分析petgraph调用关系
    fn _analyze_petgraph_call_relations(&self, code_graph: &mut PetCodeGraph) {
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
                            
                            if let Some(caller_info) = code_graph.get_function_by_id(caller_id) {
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
                                if let Err(e) = code_graph.add_call_relation(relation) {
                                    warn!("Failed to add call relation: {}", e);
                                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::treesitter::ast_instance_structs::{FunctionDeclaration, FunctionArg, TypeDef, AstSymbolFields, AstSymbolInstance};
    use crate::ast::treesitter::language_id::LanguageId;
    use crate::ast::treesitter::structs::SymbolType;
    use std::path::PathBuf;
    use std::any::Any;
    use uuid::Uuid;
    use tree_sitter::Range;
    use tree_sitter::Point;

    // 创建一个简单的测试函数，直接使用FunctionDeclaration
    fn create_test_function(name: &str, language: LanguageId, args: Vec<FunctionArg>, return_type: Option<TypeDef>) -> FunctionDeclaration {
        let fields = AstSymbolFields {
            guid: Uuid::new_v4(),
            name: name.to_string(),
            language,
            file_path: PathBuf::from("test.rs"),
            namespace: "test".to_string(),
            parent_guid: None,
            childs_guid: vec![],
            full_range: Range {
                start_point: Point { row: 0, column: 0 },
                end_point: Point { row: 0, column: 0 },
                start_byte: 0,
                end_byte: 0,
            },
            declaration_range: Range {
                start_point: Point { row: 0, column: 0 },
                end_point: Point { row: 0, column: 0 },
                start_byte: 0,
                end_byte: 0,
            },
            definition_range: Range {
                start_point: Point { row: 0, column: 0 },
                end_point: Point { row: 0, column: 0 },
                start_byte: 0,
                end_byte: 0,
            },
            linked_decl_guid: None,
            linked_decl_type: None,
            caller_guid: None,
            is_error: false,
            caller_depth: None,
        };

        FunctionDeclaration {
            ast_fields: fields,
            template_types: vec![],
            args,
            return_type,
        }
    }

    fn create_mock_function(name: &str, language: LanguageId, args: Vec<FunctionArg>, return_type: Option<TypeDef>) -> FunctionDeclaration {
        create_test_function(name, language, args, return_type)
    }

    #[test]
    fn test_rust_function_signature() {
        let parser = CodeParser::new();
        
        let args = vec![
            FunctionArg {
                name: "x".to_string(),
                type_: Some(TypeDef {
                    name: Some("i32".to_string()),
                    ..Default::default()
                }),
            },
            FunctionArg {
                name: "y".to_string(),
                type_: Some(TypeDef {
                    name: Some("String".to_string()),
                    ..Default::default()
                }),
            },
        ];

        let return_type = Some(TypeDef {
            name: Some("bool".to_string()),
            ..Default::default()
        });

        let mock_symbol = create_mock_function("test_function", LanguageId::Rust, args, return_type);
        
        let signature = parser._extract_function_signature(&mock_symbol);
        assert!(signature.is_some());
        assert_eq!(signature.unwrap(), "fn test_function(x: i32, y: String) -> bool");
    }

    #[test]
    fn test_python_function_signature() {
        let parser = CodeParser::new();
        
        let args = vec![
            FunctionArg {
                name: "name".to_string(),
                type_: Some(TypeDef {
                    name: Some("str".to_string()),
                    ..Default::default()
                }),
            },
            FunctionArg {
                name: "age".to_string(),
                type_: Some(TypeDef {
                    name: Some("int".to_string()),
                    ..Default::default()
                }),
            },
        ];

        let return_type = Some(TypeDef {
            name: Some("str".to_string()),
            ..Default::default()
        });

        let mock_symbol = create_mock_function("greet", LanguageId::Python, args, return_type);
        
        let signature = parser._extract_function_signature(&mock_symbol);
        assert!(signature.is_some());
        assert_eq!(signature.unwrap(), "def greet(name: str, age: int) -> str:");
    }

    #[test]
    fn test_function_with_template_params() {
        let parser = CodeParser::new();
        
        let args = vec![
            FunctionArg {
                name: "value".to_string(),
                type_: Some(TypeDef {
                    name: Some("T".to_string()),
                    ..Default::default()
                }),
            },
        ];

        let template_types = vec![
            TypeDef {
                name: Some("T".to_string()),
                ..Default::default()
            },
        ];

        let mut mock_symbol = create_mock_function("process", LanguageId::Rust, args, None);
        mock_symbol.template_types = template_types;
        
        let signature = parser._extract_function_signature(&mock_symbol);
        assert!(signature.is_some());
        assert_eq!(signature.unwrap(), "fn process<T>(value: T)");
    }

    #[test]
    fn test_function_with_empty_args() {
        let parser = CodeParser::new();
        
        let mock_symbol = create_mock_function("main", LanguageId::Rust, vec![], None);
        
        let signature = parser._extract_function_signature(&mock_symbol);
        assert!(signature.is_some());
        assert_eq!(signature.unwrap(), "fn main()");
    }

    #[test]
    fn test_function_with_unknown_types() {
        let parser = CodeParser::new();
        
        let args = vec![
            FunctionArg {
                name: "param".to_string(),
                type_: None, // 未知类型
            },
        ];

        let mock_symbol = create_mock_function("unknown", LanguageId::Rust, args, None);
        
        let signature = parser._extract_function_signature(&mock_symbol);
        assert!(signature.is_some());
        assert_eq!(signature.unwrap(), "fn unknown(param: _)");
    }

    #[test]
    fn test_safe_type_name() {
        let parser = CodeParser::new();
        
        // 测试有名称的类型
        let type_def = TypeDef {
            name: Some("String".to_string()),
            ..Default::default()
        };
        assert_eq!(parser._safe_type_name(&type_def), "String");
        
        // 测试只有推理信息的类型
        let type_def = TypeDef {
            name: None,
            inference_info: Some("inferred_type".to_string()),
            ..Default::default()
        };
        assert_eq!(parser._safe_type_name(&type_def), "inferred_type");
        
        // 测试完全未知的类型
        let type_def = TypeDef {
            name: None,
            inference_info: None,
            ..Default::default()
        };
        assert_eq!(parser._safe_type_name(&type_def), "unknown");
    }

    #[test]
    fn test_escape_identifier() {
        let parser = CodeParser::new();
        
        // 测试普通标识符
        assert_eq!(parser._escape_identifier("normal_name"), "normal_name");
        
        // 测试包含特殊字符的标识符
        assert_eq!(parser._escape_identifier("name with spaces"), "name with spaces");
        assert_eq!(parser._escape_identifier("name\"with\"quotes"), "name\\\"with\\\"quotes");
    }
} 