use crate::builtins::PyTypeRef;
use crate::pyobject::{PyValue, StaticType,PyResult};
use crate::VirtualMachine;

#[pyimpl]
pub trait CDataObject: PyValue {
    // A lot of the logic goes in this trait
    // There's also other traits that should have different implementations for some functions
    // present here
}

#[pyclass(module = "_ctypes", name = "_CData")]
#[derive(Debug)]
pub struct PyCData {

}

impl PyValue for PyCData {
    fn class(vm: &VirtualMachine) -> &PyTypeRef {
        Self::static_type()
    }
}

#[pyimpl]
impl PyCData {
    #[inline]
    pub fn new() -> PyCData {
        PyCData {
            
        }
    }

    #[pymethod(name = "__init__")]
    fn init(&self, vm: &VirtualMachine) -> PyResult<()> {
        Ok(())
    }

}


impl CDataObject for PyCData {

}