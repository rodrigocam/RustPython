use crate::pyobject::PyObjectRef;
use crate::VirtualMachine;

pub(crate) fn make_module(vm: &VirtualMachine) -> PyObjectRef {
    let module = _ctypes::make_module(vm);
    module
}


#[pymodule]
mod _ctypes {
    extern crate libloading;

    #[pyfunction]
    fn hello(_a: i32) {
        println!("Hello ctypes");
    }
}
