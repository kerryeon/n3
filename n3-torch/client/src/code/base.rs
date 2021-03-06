use pyo3::prelude::*;
use pyo3::types::PyDict;

#[derive(Debug)]
pub struct BuildArgs<'a> {
    pub args: &'a PyDict,
    pub scripts: &'a n3_program::PythonScripts,
}

pub trait BuildCode<'a> {
    type Args;
    type Output;

    fn build(&'a self, py: Python<'a>, args: Self::Args) -> PyResult<Self::Output>;
}

impl<'a> BuildCode<'a> for n3_program::Code {
    type Args = &'a BuildArgs<'a>;
    type Output = &'a PyAny;

    fn build(&'a self, py: Python<'a>, args: Self::Args) -> PyResult<Self::Output> {
        match self {
            Self::Node(node) => node.build(py, args),
            Self::Extern(node) => node.build(py, args),
        }
    }
}
