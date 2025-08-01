use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::codegraph::types::{PetCodeGraph, FunctionInfo, CallRelation, CodeGraphStats, ParameterInfo};

/// petgraph代码图存储格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PetGraphStorage {
    /// 函数信息列表
    pub functions: Vec<FunctionInfo>,
    /// 调用关系列表
    pub call_relations: Vec<CallRelation>,
    /// 函数名映射
    pub function_names: HashMap<String, Vec<Uuid>>,
    /// 文件映射
    pub file_functions: HashMap<PathBuf, Vec<Uuid>>,
    /// 统计信息
    pub stats: CodeGraphStats,
}

impl PetGraphStorage {
    /// 从PetCodeGraph创建存储格式
    pub fn from_petgraph(code_graph: &PetCodeGraph) -> Self {
        let functions: Vec<FunctionInfo> = code_graph.get_all_functions().into_iter().cloned().collect();
        let call_relations: Vec<CallRelation> = code_graph.get_all_call_relations().into_iter().cloned().collect();
        
        Self {
            functions,
            call_relations,
            function_names: code_graph.function_names.clone(),
            file_functions: code_graph.file_functions.clone(),
            stats: code_graph.stats.clone(),
        }
    }

    /// 转换为PetCodeGraph
    pub fn to_petgraph(&self) -> PetCodeGraph {
        let mut code_graph = PetCodeGraph::new();
        
        // 添加所有函数
        for function in &self.functions {
            code_graph.add_function(function.clone());
        }
        
        // 添加所有调用关系
        for relation in &self.call_relations {
            if let Err(e) = code_graph.add_call_relation(relation.clone()) {
                eprintln!("Warning: Failed to add call relation: {}", e);
            }
        }
        
        // 恢复映射和统计信息
        code_graph.function_names = self.function_names.clone();
        code_graph.file_functions = self.file_functions.clone();
        code_graph.stats = self.stats.clone();
        
        code_graph
    }
}

/// petgraph代码图存储管理器
pub struct PetGraphStorageManager;

impl PetGraphStorageManager {
    /// 保存代码图到文件
    pub fn save_to_file(code_graph: &PetCodeGraph, file_path: &Path) -> Result<(), String> {
        let storage = PetGraphStorage::from_petgraph(code_graph);
        let json = serde_json::to_string_pretty(&storage)
            .map_err(|e| format!("Failed to serialize code graph: {}", e))?;
        
        fs::write(file_path, json)
            .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;
        
        Ok(())
    }

    /// 从文件加载代码图
    pub fn load_from_file(file_path: &Path) -> Result<PetCodeGraph, String> {
        let json = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;
        
        let storage: PetGraphStorage = serde_json::from_str(&json)
            .map_err(|e| format!("Failed to deserialize code graph: {}", e))?;
        
        Ok(storage.to_petgraph())
    }

    /// 保存代码图到JSON字符串
    pub fn save_to_json(code_graph: &PetCodeGraph) -> Result<String, String> {
        let storage = PetGraphStorage::from_petgraph(code_graph);
        serde_json::to_string_pretty(&storage)
            .map_err(|e| format!("Failed to serialize code graph: {}", e))
    }

    /// 从JSON字符串加载代码图
    pub fn load_from_json(json_str: &str) -> Result<PetCodeGraph, String> {
        let storage: PetGraphStorage = serde_json::from_str(json_str)
            .map_err(|e| format!("Failed to deserialize code graph: {}", e))?;
        
        Ok(storage.to_petgraph())
    }

    /// 保存代码图为二进制格式
    pub fn save_to_binary(code_graph: &PetCodeGraph, file_path: &Path) -> Result<(), String> {
        let storage = PetGraphStorage::from_petgraph(code_graph);
        let binary = bincode::serialize(&storage)
            .map_err(|e| format!("Failed to serialize code graph: {}", e))?;
        
        fs::write(file_path, binary)
            .map_err(|e| format!("Failed to write file {}: {}", file_path.display(), e))?;
        
        Ok(())
    }

    /// 从二进制文件加载代码图
    pub fn load_from_binary(file_path: &Path) -> Result<PetCodeGraph, String> {
        let binary = fs::read(file_path)
            .map_err(|e| format!("Failed to read file {}: {}", file_path.display(), e))?;
        
        let storage: PetGraphStorage = bincode::deserialize(&binary)
            .map_err(|e| format!("Failed to deserialize code graph: {}", e))?;
        
        Ok(storage.to_petgraph())
    }

    /// 导出为GraphML格式（用于可视化工具）
    pub fn export_to_graphml(code_graph: &PetCodeGraph, file_path: &Path) -> Result<(), String> {
        let mut graphml = String::new();
        graphml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        graphml.push_str("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">\n");
        
        // 定义节点属性
        graphml.push_str("  <key id=\"name\" for=\"node\" attr.name=\"name\" attr.type=\"string\"/>\n");
        graphml.push_str("  <key id=\"file\" for=\"node\" attr.name=\"file\" attr.type=\"string\"/>\n");
        graphml.push_str("  <key id=\"language\" for=\"node\" attr.name=\"language\" attr.type=\"string\"/>\n");
        graphml.push_str("  <key id=\"line_start\" for=\"node\" attr.name=\"line_start\" attr.type=\"int\"/>\n");
        graphml.push_str("  <key id=\"line_end\" for=\"node\" attr.name=\"line_end\" attr.type=\"int\"/>\n");
        
        // 定义边属性
        graphml.push_str("  <key id=\"line_number\" for=\"edge\" attr.name=\"line_number\" attr.type=\"int\"/>\n");
        graphml.push_str("  <key id=\"is_resolved\" for=\"edge\" attr.name=\"is_resolved\" attr.type=\"boolean\"/>\n");
        
        graphml.push_str("  <graph id=\"codegraph\" edgedefault=\"directed\">\n");
        
        // 添加节点
        for (node_index, function) in code_graph.graph.node_indices().zip(code_graph.graph.node_weights()) {
            graphml.push_str(&format!("    <node id=\"n{}\">\n", node_index.index()));
            graphml.push_str(&format!("      <data key=\"name\">{}</data>\n", function.name));
            graphml.push_str(&format!("      <data key=\"file\">{}</data>\n", function.file_path.display()));
            graphml.push_str(&format!("      <data key=\"language\">{}</data>\n", function.language));
            graphml.push_str(&format!("      <data key=\"line_start\">{}</data>\n", function.line_start));
            graphml.push_str(&format!("      <data key=\"line_end\">{}</data>\n", function.line_end));
            graphml.push_str("    </node>\n");
        }
        
        // 添加边
        for (edge_index, edge) in code_graph.graph.edge_indices().zip(code_graph.graph.edge_weights()) {
            if let Some((source, target)) = code_graph.graph.edge_endpoints(edge_index) {
                graphml.push_str(&format!("    <edge id=\"e{}\" source=\"n{}\" target=\"n{}\">\n", 
                    edge_index.index(), source.index(), target.index()));
                graphml.push_str(&format!("      <data key=\"line_number\">{}</data>\n", edge.line_number));
                graphml.push_str(&format!("      <data key=\"is_resolved\">{}</data>\n", edge.is_resolved));
                graphml.push_str("    </edge>\n");
            }
        }
        
        graphml.push_str("  </graph>\n");
        graphml.push_str("</graphml>\n");
        
        fs::write(file_path, graphml)
            .map_err(|e| format!("Failed to write GraphML file {}: {}", file_path.display(), e))?;
        
        Ok(())
    }

    /// 导出为GEXF格式（用于Gephi等工具）
    pub fn export_to_gexf(code_graph: &PetCodeGraph, file_path: &Path) -> Result<(), String> {
        let mut gexf = String::new();
        gexf.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        gexf.push_str("<gexf xmlns=\"http://www.gexf.net/1.3\" version=\"1.3\">\n");
        gexf.push_str("  <meta lastmodifieddate=\"2024-01-01\">\n");
        gexf.push_str("    <creator>CodeGraph Exporter</creator>\n");
        gexf.push_str("    <description>Code dependency graph</description>\n");
        gexf.push_str("  </meta>\n");
        gexf.push_str("  <graph mode=\"static\" defaultedgetype=\"directed\">\n");
        
        // 定义节点属性
        gexf.push_str("    <attributes class=\"node\">\n");
        gexf.push_str("      <attribute id=\"0\" title=\"name\" type=\"string\"/>\n");
        gexf.push_str("      <attribute id=\"1\" title=\"file\" type=\"string\"/>\n");
        gexf.push_str("      <attribute id=\"2\" title=\"language\" type=\"string\"/>\n");
        gexf.push_str("      <attribute id=\"3\" title=\"line_start\" type=\"integer\"/>\n");
        gexf.push_str("      <attribute id=\"4\" title=\"line_end\" type=\"integer\"/>\n");
        gexf.push_str("    </attributes>\n");
        
        // 定义边属性
        gexf.push_str("    <attributes class=\"edge\">\n");
        gexf.push_str("      <attribute id=\"0\" title=\"line_number\" type=\"integer\"/>\n");
        gexf.push_str("      <attribute id=\"1\" title=\"is_resolved\" type=\"boolean\"/>\n");
        gexf.push_str("    </attributes>\n");
        
        gexf.push_str("    <nodes>\n");
        
        // 添加节点
        for (node_index, function) in code_graph.graph.node_indices().zip(code_graph.graph.node_weights()) {
            gexf.push_str(&format!("      <node id=\"{}\" label=\"{}\">\n", node_index.index(), function.name));
            gexf.push_str("        <attvalues>\n");
            gexf.push_str(&format!("          <attvalue for=\"0\" value=\"{}\"/>\n", function.name));
            gexf.push_str(&format!("          <attvalue for=\"1\" value=\"{}\"/>\n", function.file_path.display()));
            gexf.push_str(&format!("          <attvalue for=\"2\" value=\"{}\"/>\n", function.language));
            gexf.push_str(&format!("          <attvalue for=\"3\" value=\"{}\"/>\n", function.line_start));
            gexf.push_str(&format!("          <attvalue for=\"4\" value=\"{}\"/>\n", function.line_end));
            gexf.push_str("        </attvalues>\n");
            gexf.push_str("      </node>\n");
        }
        
        gexf.push_str("    </nodes>\n");
        gexf.push_str("    <edges>\n");
        
        // 添加边
        for (edge_index, edge) in code_graph.graph.edge_indices().zip(code_graph.graph.edge_weights()) {
            if let Some((source, target)) = code_graph.graph.edge_endpoints(edge_index) {
                gexf.push_str(&format!("      <edge id=\"{}\" source=\"{}\" target=\"{}\">\n", 
                    edge_index.index(), source.index(), target.index()));
                gexf.push_str("        <attvalues>\n");
                gexf.push_str(&format!("          <attvalue for=\"0\" value=\"{}\"/>\n", edge.line_number));
                gexf.push_str(&format!("          <attvalue for=\"1\" value=\"{}\"/>\n", edge.is_resolved));
                gexf.push_str("        </attvalues>\n");
                gexf.push_str("      </edge>\n");
            }
        }
        
        gexf.push_str("    </edges>\n");
        gexf.push_str("  </graph>\n");
        gexf.push_str("</gexf>\n");
        
        fs::write(file_path, gexf)
            .map_err(|e| format!("Failed to write GEXF file {}: {}", file_path.display(), e))?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;
    use tempfile::TempDir;
    use std::fs;

    /// 创建测试用的 PetCodeGraph
    fn create_test_petgraph() -> PetCodeGraph {
        let mut code_graph = PetCodeGraph::new();
        
        // 创建测试函数
        let function1 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function1".to_string(),
            file_path: PathBuf::from("test.rs"),
            line_start: 1,
            line_end: 10,
            namespace: "test".to_string(),
            language: "rust".to_string(),
            signature: Some("fn test_function1()".to_string()),
            return_type: Some("()".to_string()),
            parameters: vec![],
        };
        
        let function2 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function2".to_string(),
            file_path: PathBuf::from("test.rs"),
            line_start: 12,
            line_end: 20,
            namespace: "test".to_string(),
            language: "rust".to_string(),
            signature: Some("fn test_function2()".to_string()),
            return_type: Some("()".to_string()),
            parameters: vec![],
        };
        
        let function3 = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function3".to_string(),
            file_path: PathBuf::from("lib.rs"),
            line_start: 1,
            line_end: 15,
            namespace: "lib".to_string(),
            language: "rust".to_string(),
            signature: Some("fn test_function3()".to_string()),
            return_type: Some("i32".to_string()),
            parameters: vec![],
        };
        
        let node1 = code_graph.add_function(function1.clone());
        let node2 = code_graph.add_function(function2.clone());
        let node3 = code_graph.add_function(function3.clone());
        
        // 添加调用关系
        let relation1 = CallRelation {
            caller_id: function1.id,
            callee_id: function2.id,
            caller_name: function1.name.clone(),
            callee_name: function2.name.clone(),
            caller_file: function1.file_path.clone(),
            callee_file: function2.file_path.clone(),
            line_number: 5,
            is_resolved: true,
        };
        
        let relation2 = CallRelation {
            caller_id: function2.id,
            callee_id: function3.id,
            caller_name: function2.name.clone(),
            callee_name: function3.name.clone(),
            caller_file: function2.file_path.clone(),
            callee_file: function3.file_path.clone(),
            line_number: 15,
            is_resolved: false,
        };
        
        code_graph.add_call_relation(relation1).unwrap();
        code_graph.add_call_relation(relation2).unwrap();
        
        code_graph
    }

    #[test]
    fn test_storage_roundtrip() {
        let code_graph = create_test_petgraph();
        
        // 测试存储和加载
        let storage = PetGraphStorage::from_petgraph(&code_graph);
        let loaded_graph = storage.to_petgraph();
        
        // 验证函数数量
        assert_eq!(loaded_graph.get_all_functions().len(), 3);
        assert_eq!(loaded_graph.get_all_call_relations().len(), 2);
        
        // 验证函数名映射
        assert_eq!(loaded_graph.function_names.len(), 3);
        assert!(loaded_graph.function_names.contains_key("test_function1"));
        assert!(loaded_graph.function_names.contains_key("test_function2"));
        assert!(loaded_graph.function_names.contains_key("test_function3"));
        
        // 验证文件映射
        assert_eq!(loaded_graph.file_functions.len(), 2);
        assert!(loaded_graph.file_functions.contains_key(&PathBuf::from("test.rs")));
        assert!(loaded_graph.file_functions.contains_key(&PathBuf::from("lib.rs")));
        
        // 验证统计信息
        assert_eq!(loaded_graph.stats.total_functions, 3);
        assert_eq!(loaded_graph.stats.resolved_calls, 1);
        assert_eq!(loaded_graph.stats.unresolved_calls, 1);
    }

    #[test]
    fn test_json_roundtrip() {
        let code_graph = create_test_petgraph();
        
        // 测试JSON序列化和反序列化
        let json = PetGraphStorageManager::save_to_json(&code_graph).unwrap();
        let loaded_graph = PetGraphStorageManager::load_from_json(&json).unwrap();
        
        // 验证基本结构
        assert_eq!(loaded_graph.get_all_functions().len(), 3);
        assert_eq!(loaded_graph.get_all_call_relations().len(), 2);
        
        // 验证JSON格式正确
        assert!(json.contains("test_function1"));
        assert!(json.contains("test_function2"));
        assert!(json.contains("test_function3"));
        assert!(json.contains("test.rs"));
        assert!(json.contains("lib.rs"));
    }

    #[test]
    fn test_binary_roundtrip() {
        let code_graph = create_test_petgraph();
        let temp_dir = tempfile::tempdir().unwrap();
        let binary_file = temp_dir.path().join("test_graph.bin");
        
        // 测试二进制序列化和反序列化
        PetGraphStorageManager::save_to_binary(&code_graph, &binary_file).unwrap();
        let loaded_graph = PetGraphStorageManager::load_from_binary(&binary_file).unwrap();
        
        // 验证基本结构
        assert_eq!(loaded_graph.get_all_functions().len(), 3);
        assert_eq!(loaded_graph.get_all_call_relations().len(), 2);
        
        // 验证文件存在且不为空
        assert!(binary_file.exists());
        let metadata = fs::metadata(&binary_file).unwrap();
        assert!(metadata.len() > 0);
    }

    #[test]
    fn test_file_operations() {
        let code_graph = create_test_petgraph();
        let temp_dir = tempfile::tempdir().unwrap();
        let json_file = temp_dir.path().join("test_graph.json");
        let binary_file = temp_dir.path().join("test_graph.bin");
        
        // 测试JSON文件操作
        PetGraphStorageManager::save_to_file(&code_graph, &json_file).unwrap();
        let loaded_json_graph = PetGraphStorageManager::load_from_file(&json_file).unwrap();
        
        // 测试二进制文件操作
        PetGraphStorageManager::save_to_binary(&code_graph, &binary_file).unwrap();
        let loaded_binary_graph = PetGraphStorageManager::load_from_binary(&binary_file).unwrap();
        
        // 验证两种格式加载的结果一致
        assert_eq!(loaded_json_graph.get_all_functions().len(), loaded_binary_graph.get_all_functions().len());
        assert_eq!(loaded_json_graph.get_all_call_relations().len(), loaded_binary_graph.get_all_call_relations().len());
        
        // 验证文件存在
        assert!(json_file.exists());
        assert!(binary_file.exists());
    }

    #[test]
    fn test_graphml_export() {
        let code_graph = create_test_petgraph();
        let temp_dir = tempfile::tempdir().unwrap();
        let graphml_file = temp_dir.path().join("test_graph.graphml");
        
        // 测试GraphML导出
        PetGraphStorageManager::export_to_graphml(&code_graph, &graphml_file).unwrap();
        
        // 验证文件存在且不为空
        assert!(graphml_file.exists());
        let metadata = fs::metadata(&graphml_file).unwrap();
        assert!(metadata.len() > 0);
        
        // 验证GraphML格式
        let content = fs::read_to_string(&graphml_file).unwrap();
        assert!(content.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(content.contains("<graphml xmlns=\"http://graphml.graphdrawing.org/xmlns\">"));
        assert!(content.contains("test_function1"));
        assert!(content.contains("test_function2"));
        assert!(content.contains("test_function3"));
    }

    #[test]
    fn test_gexf_export() {
        let code_graph = create_test_petgraph();
        let temp_dir = tempfile::tempdir().unwrap();
        let gexf_file = temp_dir.path().join("test_graph.gexf");
        
        // 测试GEXF导出
        PetGraphStorageManager::export_to_gexf(&code_graph, &gexf_file).unwrap();
        
        // 验证文件存在且不为空
        assert!(gexf_file.exists());
        let metadata = fs::metadata(&gexf_file).unwrap();
        assert!(metadata.len() > 0);
        
        // 验证GEXF格式
        let content = fs::read_to_string(&gexf_file).unwrap();
        assert!(content.contains("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(content.contains("<gexf xmlns=\"http://www.gexf.net/1.3\" version=\"1.3\">"));
        assert!(content.contains("test_function1"));
        assert!(content.contains("test_function2"));
        assert!(content.contains("test_function3"));
    }

    #[test]
    fn test_empty_graph_storage() {
        let empty_graph = PetCodeGraph::new();
        
        // 测试空图的存储和加载
        let storage = PetGraphStorage::from_petgraph(&empty_graph);
        let loaded_graph = storage.to_petgraph();
        
        // 验证空图结构
        assert_eq!(loaded_graph.get_all_functions().len(), 0);
        assert_eq!(loaded_graph.get_all_call_relations().len(), 0);
        assert_eq!(loaded_graph.function_names.len(), 0);
        assert_eq!(loaded_graph.file_functions.len(), 0);
        assert_eq!(loaded_graph.stats.total_functions, 0);
    }

    #[test]
    fn test_error_handling() {
        let temp_dir = tempfile::tempdir().unwrap();
        let non_existent_file = temp_dir.path().join("non_existent.json");
        
        // 测试加载不存在的文件
        let result = PetGraphStorageManager::load_from_file(&non_existent_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to read file"));
        
        // 测试加载无效的JSON
        let invalid_json_file = temp_dir.path().join("invalid.json");
        fs::write(&invalid_json_file, "invalid json content").unwrap();
        
        let result = PetGraphStorageManager::load_from_file(&invalid_json_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Failed to deserialize"));
    }

    #[test]
    fn test_complex_graph_storage() {
        let mut code_graph = PetCodeGraph::new();
        
        // 创建更复杂的图结构
        let mut functions = Vec::new();
        for i in 0..10 {
            let function = FunctionInfo {
                id: Uuid::new_v4(),
                name: format!("function_{}", i),
                file_path: PathBuf::from(format!("file_{}.rs", i % 3)),
                line_start: i * 10 + 1,
                line_end: i * 10 + 10,
                namespace: format!("namespace_{}", i % 2),
                language: "rust".to_string(),
                signature: Some(format!("fn function_{}()", i)),
                return_type: Some("()".to_string()),
                parameters: vec![],
            };
            functions.push(function);
        }
        
        // 添加所有函数
        for function in &functions {
            code_graph.add_function(function.clone());
        }
        
        // 添加调用关系（链式调用）
        for i in 0..9 {
            let relation = CallRelation {
                caller_id: functions[i].id,
                callee_id: functions[i + 1].id,
                caller_name: functions[i].name.clone(),
                callee_name: functions[i + 1].name.clone(),
                caller_file: functions[i].file_path.clone(),
                callee_file: functions[i + 1].file_path.clone(),
                line_number: i * 10 + 5,
                is_resolved: i % 2 == 0,
            };
            code_graph.add_call_relation(relation).unwrap();
        }
        
        // 测试存储和加载
        let storage = PetGraphStorage::from_petgraph(&code_graph);
        let loaded_graph = storage.to_petgraph();
        
        // 验证复杂图结构
        assert_eq!(loaded_graph.get_all_functions().len(), 10);
        assert_eq!(loaded_graph.get_all_call_relations().len(), 9);
        assert_eq!(loaded_graph.function_names.len(), 10);
        assert_eq!(loaded_graph.file_functions.len(), 3); // 3个不同的文件
        
        // 验证统计信息
        assert_eq!(loaded_graph.stats.total_functions, 10);
        assert_eq!(loaded_graph.stats.resolved_calls, 5); // 偶数索引的调用是已解析的
        assert_eq!(loaded_graph.stats.unresolved_calls, 4); // 奇数索引的调用是未解析的
    }

    #[test]
    fn test_storage_with_parameters() {
        let mut code_graph = PetCodeGraph::new();
        
        // 创建带参数的函数
        let function = FunctionInfo {
            id: Uuid::new_v4(),
            name: "test_function".to_string(),
            file_path: PathBuf::from("test.rs"),
            line_start: 1,
            line_end: 10,
            namespace: "test".to_string(),
            language: "rust".to_string(),
            signature: Some("fn test_function(x: i32, y: String) -> i32".to_string()),
            return_type: Some("i32".to_string()),
            parameters: vec![
                ParameterInfo {
                    name: "x".to_string(),
                    type_name: Some("i32".to_string()),
                    default_value: None,
                },
                ParameterInfo {
                    name: "y".to_string(),
                    type_name: Some("String".to_string()),
                    default_value: Some("\"default\"".to_string()),
                },
            ],
        };
        
        code_graph.add_function(function);
        
        // 测试存储和加载
        let storage = PetGraphStorage::from_petgraph(&code_graph);
        let loaded_graph = storage.to_petgraph();
        
        // 验证参数信息被正确保存
        let functions = loaded_graph.get_all_functions();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].parameters.len(), 2);
        assert_eq!(functions[0].parameters[0].name, "x");
        assert_eq!(functions[0].parameters[0].type_name, Some("i32".to_string()));
        assert_eq!(functions[0].parameters[1].name, "y");
        assert_eq!(functions[0].parameters[1].type_name, Some("String".to_string()));
        assert_eq!(functions[0].parameters[1].default_value, Some("\"default\"".to_string()));
    }

} 