use std::convert::TryInto;
use std::{fmt, mem, os::raw::*};

use num_bigint::Sign;
use rustpython_common::borrow::BorrowValue;
use widestring::{WideCString, WideChar};

use crate::builtins::memory::try_buffer_from_object;
use crate::builtins::{PyBytes, PyInt, PyStr, PyTypeRef};
use crate::common::lock::PyRwLock;
use crate::function::FuncArgs;
use crate::pyobject::{
    PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::{PyCData, RawBuffer};
use crate::stdlib::ctypes::pointer::PyCPointer;
use crate::stdlib::ctypes::primitive::PySimpleType;

macro_rules! os_match_type {
    (
        $kind: expr,

        $(
            $($type: literal)|+ => $body: ident
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

pub fn make_array_with_lenght(
    cls: PyTypeRef,
    length: usize,
    vm: &VirtualMachine,
) -> PyResult<PyRef<PyCArray>> {
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

        make_array_with_lenght(cls, length, vm)
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
                unsafe {
                    if cfg!(windows) {
                        WideCString::from_vec_with_nul_unchecked(
                            buffer
                                .obj_bytes()
                                .chunks_exact(2)
                                .map(|c| {
                                    let chunk: [u8; 2] = c.try_into().unwrap();
                                    u16::from_ne_bytes(chunk) as u32
                                })
                                .collect::<Vec<u32>>(),
                        )
                    } else {
                        WideCString::from_vec_with_nul_unchecked(
                            buffer
                                .obj_bytes()
                                .chunks(4)
                                .map(|c| {
                                    let chunk: [u8; 4] = c.try_into().unwrap();
                                    u32::from_ne_bytes(chunk)
                                })
                                .collect::<Vec<u32>>(),
                        )
                    }
                }
                .to_string()
                .map_err(|e| vm.new_runtime_error(e.to_string()))?,
            )
        } else {
            // self._type_ == "c"
            let bytes = buffer.obj_bytes();

            let bytes_inner = if let Some((last, elements)) = bytes.split_last() {
                if *last == 0 {
                    elements.to_vec()
                } else {
                    bytes.to_vec()
                }
            } else {
                vec![0; 0]
            };

            PyBytes::from(bytes_inner).into_object(vm)
        };

        Ok(res)
    }

    #[pyproperty(name = "value", setter)]
    fn set_value(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        let obj = self.into_object(vm);
        let buffer = try_buffer_from_object(vm, &obj)?;
        let my_size = buffer.get_options().len;
        let mut bytes = buffer.obj_bytes_mut();

        if self._type_ == "c" {
            // bytes
            if let Ok(value) = value.downcast_exact::<PyBytes>(vm) {
                let wide_bytes = value.to_vec();

                if wide_bytes.len() > my_size {
                    Err(vm.new_value_error("byte string too long".to_string()))
                } else {
                    bytes[0..wide_bytes.len()].copy_from_slice(wide_bytes.as_slice());
                    if wide_bytes.len() < my_size {
                        bytes[my_size] = 0;
                    }
                    Ok(())
                }
            } else {
                Err(vm.new_value_error(format!(
                    "bytes expected instead of {} instance",
                    value.class().name
                )))
            }
        } else {
            // unicode string self._type_ == "u"
            if let Ok(value) = value.downcast_exact::<PyStr>(vm) {
                let mut wide_str =
                    unsafe { WideCString::from_str_with_nul_unchecked(value.to_string()) };

                if wide_str.len() > my_size {
                    Err(vm.new_value_error("string too long".to_string()))
                } else {
                    let res = if cfg!(windows) {
                        wide_str
                            .into_vec()
                            .iter_mut()
                            .map(|i| u16::to_ne_bytes(*i as u16).to_vec())
                            .flatten()
                            .collect::<Vec<u8>>()
                    } else {
                        wide_str
                            .into_vec()
                            .iter_mut()
                            .map(|i| u32::to_ne_bytes(*i).to_vec())
                            .flatten()
                            .collect::<Vec<u8>>()
                    };

                    bytes[0..wide_str.len()].copy_from_slice(res.as_slice());

                    Ok(())
                }
            } else {
                Err(vm.new_value_error(format!(
                    "unicode string expected instead of {} instance",
                    value.class().name
                )))
            }
        }
    }

    #[pyproperty(name = "raw")]
    pub fn raw(&self, vm: &VirtualMachine) -> PyResult<PyObjectRef> {
        // self._type_ == "c"

        let obj = self.into_object(vm);
        let buffer = try_buffer_from_object(vm, &obj)?;

        Ok(PyBytes::from(buffer.obj_bytes().to_vec()).into_object(vm))
    }

    #[pyproperty(name = "raw", setter)]
    fn set_raw(&self, value: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        let obj = self.into_object(vm);
        let my_buffer = try_buffer_from_object(vm, &obj)?;
        let my_size = my_buffer.get_options().len;

        let new_value = try_buffer_from_object(vm, &value)?;
        let new_size = new_value.get_options().len;

        // byte string self._type_ == "c"
        if new_size > my_size {
            Err(vm.new_value_error("byte string too long".to_string()))
        } else {
            let mut borrowed_buffer = my_buffer.obj_bytes_mut();
            let src = new_value.obj_bytes();
            borrowed_buffer[0..new_size].copy_from_slice(&src);
            Ok(())
        }
    }
}
