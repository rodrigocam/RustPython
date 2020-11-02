use crate::builtins::tuple::PyTupleRef;
use crate::builtins::PyTypeRef;
use crate::pyobject::{PyValue, StaticType, PyResult, PyObjectRef};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::CDataObject;

#[pyclass(module = "_ctypes", name = "CFuncPtr")]
#[derive(Debug)]
pub struct PyCFuncPtr {
    // Replace both PyObjectRef and PyTupleRef to something that implements the trait CDataObject
    // Ideally this would be PyRef<dyn CDataObject>
    // But there's some implementations to be done in order to make this happen

    _argtypes_: Vec<PyTupleRef>, 
    _restype_: Option<PyObjectRef>,
    // ext_func: extern "C" fn(),
}

impl PyValue for PyCFuncPtr {
    fn class(vm: &VirtualMachine) -> &PyTypeRef {
        Self::static_type()
    }
}

#[pyimpl]
impl PyCFuncPtr {
    #[inline]
    pub fn new() -> PyCFuncPtr {
        // PyCFuncPtr {
        //     _argtypes_: argtypes.map_or(Vec::new(), convert_from_tuple),
        //     _restype_: restype.and_then(convert_from_pytype)
        // }
        PyCFuncPtr {
            _argtypes_: Vec::new(),
            _restype_: None
        }
    }
    
    // #[pyproperty]
    // pub fn _argtypes_(&self) -> Option<PyTupleRef> {
    //     // I think this needs to return a tuple reference to the objects that have CDataObject implementations
    //     // This kind off also wrong in the CPython's way, they allow classes with _as_parameter_ object attribute...
    //     convert_to_tuple(self._argtypes_)    
    // }

    // #[pyproperty(setter)]
    // pub fn set__argtypes_(&self, argtypes: PyTupleRef) {
    //     self._argtypes_ = convert_from_tuple(argtypes)
    // }

    #[pymethod(name = "__call__")]
    pub fn call(&self) {

    }
}


impl CDataObject for PyCFuncPtr {

}