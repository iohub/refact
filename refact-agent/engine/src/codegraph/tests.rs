#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::fs;
    use std::collections::HashMap;
    use uuid::Uuid;
    use tempfile::TempDir;

    use crate::codegraph::{
        CodeGraphAnalyzer, CodeGraph, FunctionInfo, CallRelation,
        types::{RelationType, CodeGraphStats, ParameterInfo}
    };

    /// 创建测试用的临时目录和文件
    fn create_test_files() -> TempDir {
        let temp_dir = tempfile::tempdir().unwrap();
        
        // 创建Python测试文件
        let python_code = r#"
def main():
    result = calculate(10, 20)
    print(result)

def calculate(a, b):
    return add(a, b)

def add(x, y):
    return x + y

def unused_function():
    pass
"#;
        fs::write(temp_dir.path().join("test.py"), python_code).unwrap();

        // 创建Rust测试文件
        let rust_code = r#"
pub fn main() {
    let result = calculate(10, 20);
    println!("{}", result);
}

fn calculate(a: i32, b: i32) -> i32 {
    add(a, b)
}

fn add(x: i32, y: i32) -> i32 {
    x + y
}

fn unused_function() {
    // This function is not called
}
"#;
        fs::write(temp_dir.path().join("test.rs"), rust_code).unwrap();

        // 创建JavaScript测试文件
        let js_code = r#"
function main() {
    const result = calculate(10, 20);
    console.log(result);
}

function calculate(a, b) {
    return add(a, b);
}

function add(x, y) {
    return x + y;
}

function unusedFunction() {
    // This function is not called
}
"#;
        fs::write(temp_dir.path().join("test.js"), js_code).unwrap();

        temp_dir
    }

    #[test]
    fn test_code_graph_creation() {
        let mut code_graph = CodeGraph::new();
        
        // 创建测试函数
        let function1 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function".to_string(),
            file_path: "test.rs".into(),
            line_start: 1,
            line_end: 10,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: Some("fn test_function()".to_string()),
            return_type: Some("i32".to_string()),
            parameters: vec![
                ParameterInfo {
                    name: "x".to_string(),
                    type_name: Some("i32".to_string()),
                    default_value: None,
                }
            ],
        };

        let function2 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "another_function".to_string(),
            file_path: "test.rs".into(),
            line_start: 12,
            line_end: 20,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: Some("fn another_function()".to_string()),
            return_type: Some("String".to_string()),
            parameters: vec![],
        };

        // 添加函数到图
        code_graph.add_function(function1.clone());
        code_graph.add_function(function2.clone());

        // 验证函数被正确添加
        assert_eq!(code_graph.functions.len(), 2);
        assert!(code_graph.functions.contains_key(&function1.id));
        assert!(code_graph.functions.contains_key(&function2.id));

        // 验证函数名映射
        assert_eq!(code_graph.function_names.get("test_function").unwrap().len(), 1);
        assert_eq!(code_graph.function_names.get("another_function").unwrap().len(), 1);
    }

    #[test]
    fn test_call_relation_creation() {
        let mut code_graph = CodeGraph::new();
        
        let caller_id = Uuid::new_v4();
        let callee_id = Uuid::new_v4();

        let relation = CallRelation {
            caller_id,
            callee_id,
            caller_name: "main".to_string(),
            callee_name: "calculate".to_string(),
            caller_file: "test.rs".into(),
            callee_file: "test.rs".into(),
            line_number: 3,
            is_resolved: true,
        };

        code_graph.add_call_relation(relation);

        assert_eq!(code_graph.call_relations.len(), 1);
        assert_eq!(code_graph.stats.resolved_calls, 1);
        assert_eq!(code_graph.stats.unresolved_calls, 0);
    }

    #[test]
    fn test_function_lookup() {
        let mut code_graph = CodeGraph::new();
        
        let function = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function".to_string(),
            file_path: "test.rs".into(),
            line_start: 1,
            line_end: 10,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: None,
            return_type: None,
            parameters: vec![],
        };

        code_graph.add_function(function.clone());

        // 测试按名称查找
        let found_functions = code_graph.find_functions_by_name("test_function");
        assert_eq!(found_functions.len(), 1);
        assert_eq!(found_functions[0].name, "test_function");

        // 测试按文件查找
        let file_functions = code_graph.find_functions_by_file(&"test.rs".into());
        assert_eq!(file_functions.len(), 1);
        assert_eq!(file_functions[0].name, "test_function");
    }

    #[test]
    fn test_mermaid_export() {
        let mut code_graph = CodeGraph::new();
        
        let function1 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "main".to_string(),
            file_path: "test.rs".into(),
            line_start: 1,
            line_end: 5,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: None,
            return_type: None,
            parameters: vec![],
        };

        let function2 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "calculate".to_string(),
            file_path: "test.rs".into(),
            line_start: 7,
            line_end: 12,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: None,
            return_type: None,
            parameters: vec![],
        };

        code_graph.add_function(function1.clone());
        code_graph.add_function(function2.clone());

        let relation = CallRelation {
            caller_id: function1.id,
            callee_id: function2.id,
            caller_name: "main".to_string(),
            callee_name: "calculate".to_string(),
            caller_file: "test.rs".into(),
            callee_file: "test.rs".into(),
            line_number: 3,
            is_resolved: true,
        };

        code_graph.add_call_relation(relation);

        let mermaid = code_graph.to_mermaid();
        
        // 验证Mermaid格式包含必要元素
        assert!(mermaid.contains("graph TD"));
        assert!(mermaid.contains("main"));
        assert!(mermaid.contains("calculate"));
        assert!(mermaid.contains("-->"));
    }

    #[test]
    fn test_dot_export() {
        let mut code_graph = CodeGraph::new();
        
        let function = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function".to_string(),
            file_path: "test.rs".into(),
            line_start: 1,
            line_end: 10,
            namespace: "".to_string(),
            language: "rust".to_string(),
            signature: None,
            return_type: None,
            parameters: vec![],
        };

        code_graph.add_function(function);

        let dot = code_graph.to_dot();
        
        // 验证DOT格式包含必要元素
        assert!(dot.contains("digraph CodeGraph"));
        assert!(dot.contains("rankdir=TB"));
        assert!(dot.contains("test_function"));
    }

    #[test]
    fn test_analyzer_basic_functionality() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        // 分析目录
        let result = analyzer.analyze_directory(temp_dir.path());
        assert!(result.is_ok());
        
        // 验证代码图被创建
        assert!(analyzer.get_code_graph().is_some());
        
        // 验证统计信息
        let stats = analyzer.get_stats().unwrap();
        assert!(stats.total_functions > 0);
        assert!(stats.total_files > 0);
    }

    #[test]
    fn test_analyzer_call_chains() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试调用链分析
        let chains = analyzer.find_call_chains("main", 3);
        
        // 验证调用链分析工作正常
        assert!(!chains.is_empty());
    }

    #[test]
    fn test_analyzer_circular_dependencies() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试循环依赖检测
        let cycles = analyzer.find_circular_dependencies();
        
        // 对于我们的测试代码，应该没有循环依赖
        assert!(cycles.is_empty());
    }

    #[test]
    fn test_analyzer_complexity_analysis() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试复杂度分析
        let complex_functions = analyzer.find_most_complex_functions(5);
        let leaf_functions = analyzer.find_leaf_functions();
        let root_functions = analyzer.find_root_functions();
        
        // 验证分析结果
        assert!(!complex_functions.is_empty());
        assert!(!leaf_functions.is_empty());
        assert!(!root_functions.is_empty());
    }

    #[test]
    fn test_analyzer_distribution_analysis() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试分布分析
        let lang_distribution = analyzer.get_language_distribution();
        let file_distribution = analyzer.get_file_distribution();
        
        // 验证分布分析结果
        assert!(!lang_distribution.is_empty());
        assert!(!file_distribution.is_empty());
    }

    #[test]
    fn test_analyzer_report_generation() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试报告生成
        let report = analyzer.generate_call_report();
        
        // 验证报告包含必要信息
        assert!(report.contains("Code Graph Call Report"));
        assert!(report.contains("Total Functions"));
        assert!(report.contains("Language Distribution"));
    }

    #[test]
    fn test_analyzer_export_formats() {
        let temp_dir = create_test_files();
        let mut analyzer = CodeGraphAnalyzer::new();
        
        analyzer.analyze_directory(temp_dir.path()).unwrap();
        
        // 测试各种导出格式
        let mermaid = analyzer.export_mermaid();
        let dot = analyzer.export_dot();
        let json = analyzer.export_json();
        
        // 验证导出格式
        assert!(mermaid.is_some());
        assert!(dot.is_some());
        assert!(json.is_some());
        
        // 验证JSON格式
        if let Some(json_result) = json {
            assert!(json_result.is_ok());
        }
    }

    #[test]
    fn test_stats_default() {
        let stats = CodeGraphStats::default();
        
        assert_eq!(stats.total_functions, 0);
        assert_eq!(stats.total_files, 0);
        assert_eq!(stats.total_languages, 0);
        assert_eq!(stats.resolved_calls, 0);
        assert_eq!(stats.unresolved_calls, 0);
        assert!(stats.languages.is_empty());
    }

    #[test]
    fn test_unsupported_file_handling() {
        let temp_dir = tempfile::tempdir().unwrap();
        
        // 创建不支持的文件
        fs::write(temp_dir.path().join("test.txt"), "This is not a source file").unwrap();
        fs::write(temp_dir.path().join("test.md"), "# Markdown file").unwrap();
        
        let mut analyzer = CodeGraphAnalyzer::new();
        
        // 应该不会因为不支持的文件而失败
        let result = analyzer.analyze_directory(temp_dir.path());
        assert!(result.is_ok());
        
        // 验证没有解析到函数（因为没有支持的源文件）
        let stats = analyzer.get_stats();
        if let Some(stats) = stats {
            assert_eq!(stats.total_functions, 0);
        }
    }

    #[test]
    fn test_generate_and_print_codegraph() {
        let temp_dir = create_test_files();
        let mut parser = crate::codegraph::parser::CodeParser::new();
        // 解析目录下所有支持的源代码文件，生成 CodeGraph
        let code_graph = parser.build_code_graph(temp_dir.path()).expect("Failed to build code graph");
        // 打印 CodeGraph 的 DOT 格式
        let dot = code_graph.to_dot();
        println!("\n[CodeGraph DOT format]\n{}", dot);
        // 打印 CodeGraph 的 Mermaid 格式
        let mermaid = code_graph.to_mermaid();
        println!("\n[CodeGraph Mermaid format]\n{}", mermaid);
        // 打印函数统计信息
        let stats = code_graph.stats.clone();
        println!("\n[CodeGraph Stats]\n{:?}", stats);
        // 确保至少有一个函数被解析出来
        assert!(code_graph.functions.len() > 0);
    }
} 