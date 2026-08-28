#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct A {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct B {
    pub value: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct C {
    pub value: i64,
}
