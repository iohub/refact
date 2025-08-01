# CodeGraph with PetGraph Integration

This module provides code graph functionality using the PetGraph library for efficient graph operations and analysis.

## Features

- **PetGraph-based Code Graph**: Uses PetGraph's `DiGraph` for efficient graph operations
- **Multiple Export Formats**: JSON, Mermaid, DOT, GraphML, GEXF
- **Graph Analysis**: Cycle detection, topological sorting, strongly connected components
- **Persistent Storage**: Save and load code graphs from files
- **Binary Serialization**: Fast binary format using bincode

## Usage

### Basic Usage

```rust
use crate::codegraph::{CodeParser, PetCodeGraph, PetGraphStorageManager};
use std::path::Path;

// Create a code parser
let mut parser = CodeParser::new();

// Build a petgraph-based code graph
let code_graph = parser.build_petgraph_code_graph(Path::new("src"))?;

// Get statistics
let stats = code_graph.get_stats();
println!("Total functions: {}", stats.total_functions);
println!("Total files: {}", stats.total_files);
```

### Graph Analysis

```rust
// Check for cycles
if code_graph.has_cycles() {
    println!("Warning: Code graph contains cycles!");
    
    // Get strongly connected components
    let sccs = code_graph.strongly_connected_components();
    for scc in sccs {
        if scc.len() > 1 {
            println!("Cycle detected with {} functions", scc.len());
        }
    }
} else {
    // Try topological sorting
    match code_graph.topological_sort() {
        Ok(sorted_nodes) => {
            println!("Topological order:");
            for node_index in sorted_nodes {
                if let Some(function) = code_graph.get_function(node_index) {
                    println!("  {}", function.name);
                }
            }
        }
        Err(_) => println!("Cycle detected during topological sort"),
    }
}
```

### Function Analysis

```rust
// Find functions by name
let functions = code_graph.find_functions_by_name("main");

// Get callers of a function
let callers = code_graph.get_callers(&function_id);
for (caller, relation) in callers {
    println!("Called by {} at line {}", caller.name, relation.line_number);
}

// Get callees of a function
let callees = code_graph.get_callees(&function_id);
for (callee, relation) in callees {
    println!("Calls {} at line {}", callee.name, relation.line_number);
}

// Get call chains
let call_chains = code_graph.get_call_chain(&function_id, 3);
for chain in call_chains {
    println!("Call chain: {:?}", chain);
}
```

### Export and Visualization

```rust
// Export to JSON
let json = code_graph.to_json()?;
std::fs::write("codegraph.json", json)?;

// Export to Mermaid (for documentation)
let mermaid = code_graph.to_mermaid();
std::fs::write("codegraph.mmd", mermaid)?;

// Export to DOT (for Graphviz)
let dot = code_graph.to_dot();
std::fs::write("codegraph.dot", dot)?;

// Export to GraphML (for visualization tools)
PetGraphStorageManager::export_to_graphml(&code_graph, Path::new("codegraph.graphml"))?;

// Export to GEXF (for Gephi)
PetGraphStorageManager::export_to_gexf(&code_graph, Path::new("codegraph.gexf"))?;
```

### Persistent Storage

```rust
// Save to file
PetGraphStorageManager::save_to_file(&code_graph, Path::new("codegraph.json"))?;

// Load from file
let loaded_graph = PetGraphStorageManager::load_from_file(Path::new("codegraph.json"))?;

// Save to JSON string
let json_str = PetGraphStorageManager::save_to_json(&code_graph)?;

// Load from JSON string
let loaded_graph = PetGraphStorageManager::load_from_json(&json_str)?;

// Save to binary format (faster)
PetGraphStorageManager::save_to_binary(&code_graph, Path::new("codegraph.bin"))?;

// Load from binary format
let loaded_graph = PetGraphStorageManager::load_from_binary(Path::new("codegraph.bin"))?;
```

## Data Structures

### PetCodeGraph

The main graph structure that wraps PetGraph's `DiGraph`:

```rust
pub struct PetCodeGraph {
    pub graph: DiGraph<FunctionInfo, CallRelation>,
    pub function_to_node: HashMap<Uuid, NodeIndex>,
    pub node_to_function: HashMap<NodeIndex, Uuid>,
    pub function_names: HashMap<String, Vec<Uuid>>,
    pub file_functions: HashMap<PathBuf, Vec<Uuid>>,
    pub stats: CodeGraphStats,
}
```

### FunctionInfo

Represents a function in the code:

```rust
pub struct FunctionInfo {
    pub id: Uuid,
    pub name: String,
    pub file_path: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub namespace: String,
    pub language: String,
    pub signature: Option<String>,
    pub return_type: Option<String>,
    pub parameters: Vec<ParameterInfo>,
}
```

### CallRelation

Represents a function call relationship:

```rust
pub struct CallRelation {
    pub caller_id: Uuid,
    pub callee_id: Uuid,
    pub caller_name: String,
    pub callee_name: String,
    pub caller_file: PathBuf,
    pub callee_file: PathBuf,
    pub line_number: usize,
    pub is_resolved: bool,
}
```

## Supported Languages

The code parser supports multiple programming languages:

- **C/C++**: `.c`, `.cpp`, `.cc`, `.cxx`, `.c++`, `.h`, `.hpp`, `.hxx`, `.hh`
- **Python**: `.py`, `.py3`, `.pyx`
- **Java**: `.java`
- **JavaScript/TypeScript**: `.js`, `.jsx`, `.ts`, `.tsx`
- **Rust**: `.rs`

## Performance Benefits

Using PetGraph provides several performance benefits:

1. **Efficient Graph Operations**: O(1) node/edge access, O(V+E) for most algorithms
2. **Memory Efficient**: Compact representation of graph structure
3. **Fast Algorithms**: Built-in algorithms for cycle detection, topological sort, etc.
4. **Scalable**: Handles large codebases efficiently

## Comparison with Original CodeGraph

| Feature | Original CodeGraph | PetCodeGraph |
|---------|-------------------|-------------------|
| Graph Structure | HashMap + Vec | PetGraph DiGraph |
| Node Access | O(1) HashMap lookup | O(1) direct access |
| Edge Access | O(E) linear search | O(1) direct access |
| Cycle Detection | Manual implementation | Built-in algorithm |
| Topological Sort | Not available | Built-in algorithm |
| Memory Usage | Higher (multiple HashMaps) | Lower (compact graph) |
| Algorithm Performance | Slower | Faster |

## Migration from Original CodeGraph

To migrate from the original `CodeGraph` to `PetCodeGraph`:

1. Replace `CodeGraph::new()` with `PetCodeGraph::new()`
2. Use `build_petgraph_code_graph()` instead of `build_code_graph()`
3. Update method calls to use the new API
4. Take advantage of new graph analysis features

The API is designed to be similar, so most existing code should work with minimal changes. 