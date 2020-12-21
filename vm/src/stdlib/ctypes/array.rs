use std::{fmt, mem, os::raw};

use crossbeam_utils::atomic::AtomicCell;
use num_bigint::Sign;
use rustpython_common::borrow::BorrowValue;

use crate::builtins::{PyInt, PyStr, PyTypeRef};
use crate::function::FuncArgs;
use crate::pyobject::{
    IdProtocol, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::PyCData;
use crate::stdlib::ctypes::pointer::PyCPointer;
use crate::stdlib::ctypes::primitive::PySimpleType;

macro_rules! os_match_type {
    (
        $kind: expr,

        $(
            $($type: tt)|+ => $body: ident
        )+
    ) => {
        match $kind {
            $(
                $(
                    t if t == $type => { mem::size_of::<raw::$body>() }
                )+
            )+
            _ => unreachable!()
        }
    }
}

fn get_size(ty: &str) -> usize {
    os_match_type!(
        ty,
        "c" | "b" => c_schar
        "u" => c_int
        "h" => c_short
        "H" => c_ushort
        "i" => c_int
        "I" => c_uint
        "l" => c_long
        "q" => c_longlong
        "L" => c_ulong
        "Q" => c_ulonglong
        "f" => c_float
        "d" | "g" => c_double
        "?" | "B" => c_uchar
        "z" | "Z" => c_void
    )
}

fn new_array_type(cls: &PyTypeRef, vm: &VirtualMachine) -> PyResult<PyCArray> {
    let mut length = match vm.get_attribute(cls.as_object().to_owned(), "_length_") {
        Ok(length_obj) => {
            if let Ok(length_int) = length_obj.downcast_exact::<PyInt>(vm) {
                if length_int.borrow_value().sign() == Sign::Minus {
                    Err(vm.new_value_error(
                        "The '_length_' attribute must not be negative".to_string(),
                    ))
                } else {
                    Ok(
                        usize::try_from_object(vm, length_obj).or(Err(vm.new_overflow_error(
                            "The '_length_' attribute is too large".to_string(),
                        )))?,
                    )
                }
            } else {
                Err(vm.new_type_error("The '_length_' attribute must be an integer".to_string()))
            }
        }
        Err(_) => Err(vm.new_attribute_error("class must define a '_type_' _length_".to_string())),
    }?;

    if let Ok(outer_type) = vm.get_attribute(cls.as_object().to_owned(), "_type_") {
        match vm.get_attribute(outer_type, "_type_") {
            Ok(inner_type)
                if vm.issubclass(&inner_type.clone_class(), &PyCPointer::static_type())?
                    || vm
                        .issubclass(&inner_type.clone_class(), &PySimpleType::static_type())? =>
            {
                let subletter = vm
                    .get_attribute(outer_type, "_type_")?
                    .downcast_exact::<PyStr>(vm)
                    .unwrap()
                    .to_string();

                let itemsize = if subletter == "P".to_string() {
                    length = 0;
                    0
                } else {
                    get_size(subletter.as_str())
                };

                return Ok(PyCArray {
                    _type_: subletter,
                    _length_: length,
                    value: AtomicCell::new(Vec::with_capacity(length * itemsize)),
                });
            }
            Err(_) => Err(vm.new_type_error("_type_ must have storage info".to_string())),
        }
    } else {
        Err(vm.new_attribute_error("class must define a '_type_' attribute".to_string()))
    }?
}

#[pyclass(module = "_ctypes", name = "Array", base = "PyCData")]
pub struct PyCArray {
    _type_: String,
    _length_: usize,
    value: AtomicCell<Vec<u8>>,
}

impl fmt::Debug for PyCArray {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "PyCArray {{ {} {} }}",
            self._type_.as_str(),
            self._length_
        )
    }
}

impl PyValue for PyCArray {
    fn class(_vm: &VirtualMachine) -> &PyTypeRef {
        Self::static_type()
    }
}

#[pyimpl(flags(BASETYPE))]
impl PyCArray {
    #[pyslot]
    fn tp_new(cls: PyTypeRef, vm: &VirtualMachine) -> PyResult<PyRef<Self>> {
        new_array_type(&cls, vm)?.into_ref_with_type(vm, cls)
    }

    #[pymethod(magic)]
    pub fn init(&self, value: FuncArgs, vm: &VirtualMachine) -> PyResult<()> {
        Ok(())
    }

    #[pyproperty(name = "value")]
    pub fn value(&self) -> PyObjectRef {

    }

    #[pyproperty(name = "value", setter)]
    fn set_value(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        Ok(())
    }

    #[pyproperty(name = "raw")]
    pub fn raw(&self) -> PyObjectRef {
    }

    #[pyproperty(name = "raw", setter)]
    fn set_raw(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        Ok(())
    }
}
