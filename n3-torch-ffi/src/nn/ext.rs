use pyo3::prelude::*;
use pyo3::types::PyDict;

#[pyclass(subclass)]
pub struct ExternNode {
    #[pyo3(get)]
    node_input: Py<PyDict>,
    #[pyo3(get)]
    node_output: Py<PyDict>,
}

#[pymethods]
impl ExternNode {
    #[new]
    pub fn new(py: Python) -> Self {
        Self {
            node_input: PyDict::new(py).into_py(py),
            node_output: PyDict::new(py).into_py(py),
        }
    }

    pub fn init_node(&mut self, node_input: Py<PyDict>, node_output: Py<PyDict>) {
        self.node_input = node_input;
        self.node_output = node_output;
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use pyo3::types::IntoPyDict;
    use pyo3::*;

    use super::*;
    use crate::machine::*;
    use crate::PyInit_n3;

    #[test]
    fn test_subclass() -> Result<(), ()> {
        #[pyfunction]
        fn test_ext_subclass(py: Python) -> PyResult<()> {
            let mut machine: Machine = GenericMachine::new(py).into();
            let builtins = py.import("builtins")?.into_py(py);
            let torch = Torch(py);

            let n3 = wrap_pymodule!(n3)(py);
            let nn = torch.this("nn")?.into_py(py);
            let zeros = torch.this("zeros")?.into_py(py);

            // get a sample tensor graph
            py.run(
                r#"
class MyExternNode(n3.ExternNode, nn.Module):
    def __init__(self):
        super(n3.ExternNode, self).__init__()
        super(nn.Module, self).__init__()

        self.inner = nn.Linear(32, 10)

    def forward(self, x):
        return self.inner(x)


node = MyExternNode()
node.init_node({}, {})

x = zeros(3, 32)
y = node(x)
assert y.shape == (3, 10)

"#,
                Some(
                    [
                        ("__builtins__", builtins),
                        ("nn", nn),
                        ("n3", n3),
                        ("zeros", zeros),
                    ]
                    .into_py_dict(py),
                ),
                None,
            )?;

            machine.terminate()
        }

        Python::with_gil(|py| {
            let mut process = pyo3_mp::Process::new(py)?;
            process.spawn(wrap_pyfunction!(test_ext_subclass)(py)?, (), None)?;
            process.join()
        })
        .map_err(|e: PyErr| Python::with_gil(|py| e.print_and_set_sys_last_vars(py)))
    }
}
