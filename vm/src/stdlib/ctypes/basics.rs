use std::{fmt, os::raw::c_void, slice};

use crate::builtins::bytearray::PyByteArray;
use crate::builtins::int::PyInt;
use crate::builtins::memory::{Buffer, BufferOptions};
use crate::builtins::pystr::PyStrRef;
use crate::builtins::pytype::PyTypeRef;
use crate::common::borrow::{BorrowedValue, BorrowedValueMut};
use crate::function::OptionalArg;
use crate::pyobject::{
    PyObjectRc, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject,
};
use crate::VirtualMachine;

use crossbeam_utils::atomic::AtomicCell;

// GenericPyCData_new -> PyResult<PyObjectRef>
pub fn generic_pycdata_new(type_: PyTypeRef, vm: &VirtualMachine) {
    // @TODO: To be used on several places
}

fn at_address(cls: &PyTypeRef, buf: usize, vm: &VirtualMachine) -> PyResult<Vec<u8>> {
    match vm.get_attribute(cls.as_object().to_owned(), "__abstract__") {
        Ok(attr) => match bool::try_from_object(vm, attr) {
            Ok(b) if b => {
                let len = vm
                    .get_attribute(cls.as_object().to_owned(), "_length_")
                    .map_or(Ok(1), |o: PyObjectRc| {
                        match i64::try_from_object(vm, o.clone()) {
                            Ok(v_int) => {
                                if v_int < 0 {
                                    Err(vm.new_type_error("'_length_' must positive".to_string()))
                                } else {
                                    Ok(v_int as usize)
                                }
                            }
                            _ => {
                                Err(vm.new_type_error("'_length_' must be an integer".to_string()))
                            }
                        }
                    })?;

                let slice = unsafe { slice::from_raw_parts(buf as *const u8, len) };
                Ok(slice.to_vec())
            }
            Ok(_) => Err(vm.new_type_error("abstract class".to_string())),
            Err(_) => Err(vm.new_type_error("attribute '__abstract__' must be bool".to_string())),
        },
        Err(_) => {
            Err(vm.new_attribute_error("class must define a '__abstract__' attribute".to_string()))
        }
    }
}

#[pyimpl]
pub trait PyCDataMethods: PyValue {
    // A lot of the logic goes in this trait
    // There's also other traits that should have different implementations for some functions
    // present here

    // The default methods (representing CDataType_methods) here are for:
    // StructType_Type
    // UnionType_Type
    // PyCArrayType_Type
    // PyCFuncPtrType_Type

    #[pyclassmethod]
    fn from_param(cls: PyTypeRef, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<PyCData>;

    #[pyclassmethod]
    fn from_address(
        cls: PyTypeRef,
        address: PyObjectRef,
        vm: &VirtualMachine,
    ) -> PyResult<PyCData> {
        if let Ok(obj) = address.downcast_exact::<PyInt>(vm) {
            if let Ok(v) = usize::try_from_object(vm, obj.into_object()) {
                let buffer = PyByteArray::from(at_address(&cls, v, vm)?);
                Ok(PyCData::new(None, Some(buffer)))
            } else {
                Err(vm.new_runtime_error("casting pointer failed".to_string()))
            }
        } else {
            Err(vm.new_type_error("integer expected".to_string()))
        }
    }

    #[pyclassmethod]
    fn from_buffer(
        cls: PyTypeRef,
        obj: PyObjectRef,
        offset: OptionalArg,
        vm: &VirtualMachine,
    ) -> PyResult<PyCData>;

    #[pyclassmethod]
    fn from_buffer_copy(
        cls: PyTypeRef,
        obj: PyObjectRef,
        offset: OptionalArg,
        vm: &VirtualMachine,
    ) -> PyResult<PyCData>;

    #[pyclassmethod]
    fn in_dll(
        cls: PyTypeRef,
        dll: PyObjectRef,
        name: PyStrRef,
        vm: &VirtualMachine,
    ) -> PyResult<PyCData>;
}

#[pyimpl]
pub trait PyCDataSequenceMethods: PyValue {
    // CDataType_as_sequence methods are default for all *Type_Type
    // Basically the sq_repeat slot is CDataType_repeat
    // which transforms into a Array

    // #[pymethod(name = "__mul__")]
    // fn mul(&self, counter: isize, vm: &VirtualMachine) -> PyObjectRef {
    // }

    // #[pymethod(name = "__rmul__")]
    // fn rmul(&self, counter: isize, vm: &VirtualMachine) -> PyObjectRef {
    //     self.mul(counter, vm)
    // }
}

// This trait will be used by all types
pub trait PyCDataBuffer: Buffer {
    fn obj_bytes(&self) -> BorrowedValue<[u8]>;

    fn obj_bytes_mut(&self) -> BorrowedValueMut<[u8]>;

    fn release(&self);

    fn get_options(&self) -> BorrowedValue<BufferOptions>;
}

// This Trait is the equivalent of PyCData_Type on tp_base for
// Struct_Type, Union_Type, PyCPointer_Type
// PyCArray_Type, PyCSimple_Type, PyCFuncPtr_Type
#[pyclass(module = "ctypes", name = "_CData")]
pub struct PyCData {
    _objects: AtomicCell<Vec<PyObjectRc>>,
    _buffer: AtomicCell<PyByteArray>,
}

impl fmt::Debug for PyCData {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PyCData {{ _objects: {{}}, _buffer: {{}}}}",)
    }
}

impl PyValue for PyCData {
    fn class(_vm: &VirtualMachine) -> &PyTypeRef {
        Self::init_bare_type()
    }
}

impl PyCData {
    fn new(objs: Option<Vec<PyObjectRc>>, buffer: Option<PyByteArray>) -> Self {
        PyCData {
            _objects: AtomicCell::new(objs.unwrap_or(Vec::new())),
            _buffer: AtomicCell::new(buffer.unwrap_or(PyByteArray::from(Vec::new()))),
        }
    }
}

#[pyimpl]
impl PyCData {
    // PyCData_methods
    #[pymethod(name = "__ctypes_from_outparam__")]
    pub fn ctypes_from_outparam(zelf: PyRef<Self>) {}

    #[pymethod(name = "__reduce__")]
    pub fn reduce(zelf: PyRef<Self>) {}

    #[pymethod(name = "__setstate__")]
    pub fn setstate(zelf: PyRef<Self>) {}
}

// #[pyimpl]
// impl PyCDataBuffer for PyCData {

// }
