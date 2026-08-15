#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tensor {
    pub id: i64,
    pub row_values: Vec<String>,
    pub grid_values: Vec<Vec<String>>,
    pub cube_values: Vec<Vec<Vec<String>>>,
    pub hypercube_values: Vec<Vec<Vec<Vec<String>>>>,
}
