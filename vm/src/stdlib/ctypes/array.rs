use std::{fmt, mem, os::raw::*};

use byteorder::{ByteOrder, NativeEndian};
use num_bigint::Sign;
use rustpython_common::borrow::BorrowValue;
use widestring::{WideChar, WideString};

use crate::builtins::memory::try_buffer_from_object;
use crate::builtins::{PyBytes, PyInt, PyStr, PyTypeRef};
use crate::common::lock::PyRwLock;
use crate::function::FuncArgs;
use crate::pyobject::{
    IdProtocol, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::{PyCData, RawBuffer};
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
                    t if t == $type => { mem::size_of::<$body>() }
                )+
            )+
            _ => unreachable!()
        }
    }
}

fn get_size(ty: &str) -> usize {
    os_match_type!(
        ty,
        "u" => WideChar
        "c" | "b" => c_schar
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
        "P" | "z" | "Z" => c_void
    )
}

#[pyclass(module = "_ctypes", name = "Array", base = "PyCData")]
pub struct PyCArray {
    _type_: String,
    _length_: usize,
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
        let length = match vm.get_attribute(cls.as_object().to_owned(), "_length_") {
            Ok(length_obj) => {
                if let Ok(length_int) = length_obj.downcast_exact::<PyInt>(vm) {
                    if length_int.borrow_value().sign() == Sign::Minus {
                        Err(vm.new_value_error(
                            "The '_length_' attribute must not be negative".to_string(),
                        ))
                    } else {
                        Ok(usize::try_from_object(vm, length_obj)
                            .or(Err(vm.new_overflow_error(
                                "The '_length_' attribute is too large".to_string(),
                            )))?)
                    }
                } else {
                    Err(vm
                        .new_type_error("The '_length_' attribute must be an integer".to_string()))
                }
            }
            Err(_) => {
                Err(vm.new_attribute_error("class must define a '_type_' _length_".to_string()))
            }
        }?;

        if let Ok(outer_type) = vm.get_attribute(cls.as_object().to_owned(), "_type_") {
            match vm.get_attribute(outer_type, "_type_") {
                Ok(inner_type)
                    if vm.issubclass(&inner_type.clone_class(), &PyCPointer::static_type())?
                        || vm.issubclass(
                            &inner_type.clone_class(),
                            &PySimpleType::static_type(),
                        )? =>
                {
                    let subletter = vm
                        .get_attribute(outer_type, "_type_")?
                        .downcast_exact::<PyStr>(vm)
                        .unwrap()
                        .to_string();

                    let itemsize = get_size(subletter.as_str());

                    let myself = PyCArray {
                        _type_: subletter,
                        _length_: length,
                    }
                    .into_ref_with_type(vm, cls)?;

                    vm.set_attr(
                        myself.as_object(),
                        "_buffer",
                        PyRwLock::new(RawBuffer {
                            inner: Vec::with_capacity(length * itemsize).as_mut_ptr(),
                            size: length * itemsize,
                        }),
                    )?;

                    Ok(myself)
                }
                _ => Err(vm.new_type_error("_type_ must have storage info".to_string())),
            }
        } else {
            Err(vm.new_attribute_error("class must define a '_type_' attribute".to_string()))
        }
    }

    #[pymethod(magic)]
    pub fn init(&self, value: FuncArgs, vm: &VirtualMachine) -> PyResult<()> {
        // @TODO
        Ok(())
    }

    #[pyproperty(name = "value")]
    pub fn value(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        let obj = self.into_object(vm);
        let buffer = try_buffer_from_object(vm, &obj)?;

        let res = if self._type_ == "u" {
            vm.new_pyobj(
                if cfg!(windows) {
                    WideString::from_vec(
                        buffer
                            .obj_bytes()
                            .chunks(4)
                            .map(|c| NativeEndian::read_u32(c))
                            .collect::<Vec<u32>>(),
                    )
                } else {
                    WideString::from_vec(
                        buffer
                            .obj_bytes()
                            .chunks(2)
                            .map(|c| NativeEndian::read_u16(c) as u32)
                            .collect::<Vec<u32>>(),
                    )
                }
                .to_string()
                .map_err(|e| vm.new_runtime_error(e.to_string()))?,
            )
        } else {
            PyBytes::from(buffer.obj_bytes().to_vec()).into_object(vm)
        };

        Ok(res)
    }

    #[pyproperty(name = "value", setter)]
    fn set_value(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        if self._type_ == "c" {
            // bytes
        } else if self._type_ == "u" {
            // unicode string
        }

        Ok(())
    }

    #[pyproperty(name = "raw")]
    pub fn raw(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        let obj = self.into_object(vm);
        let buffer = try_buffer_from_object(vm, &obj)?;

        Ok(PyBytes::from(buffer.obj_bytes().to_vec()).into_object(vm))
    }

    #[pyproperty(name = "raw", setter)]
    fn set_raw(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        if self._type_ == "c" {
            // byte string
        } else {
        }

        Ok(())
    }
}
