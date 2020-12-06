extern crate libffi;

use std::{fmt, os::raw::*, ptr};

use crossbeam_utils::atomic::AtomicCell;

use libffi::low::{
    call as ffi_call, ffi_abi_FFI_DEFAULT_ABI as ABI, ffi_cif, ffi_type, prep_cif, CodePtr,
    Error as FFIError,
};
use libffi::middle;

use crate::builtins::pystr::PyStrRef;
use crate::builtins::{PyInt, PyTypeRef};
use crate::common::lock::PyRwLock;

use crate::function::FuncArgs;
use crate::pyobject::{
    PyObjectRc, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TryFromObject, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::basics::PyCData;
use crate::stdlib::ctypes::primitive::PySimpleType;

use crate::slots::Callable;
use crate::stdlib::ctypes::dll::dlsym;

macro_rules! ffi_type {
    ($name: ident) => {
        middle::Type::$name().as_raw_ptr()
    };
}

macro_rules! match_ffi_type {
    (
        $pointer: expr,

        $(
            $($type: ident)|+ => $body: expr
        )+
    ) => {
        match $pointer {
            $(
                $(
                    t if t == ffi_type!($type) => { $body }
                )+
            )+
            _ => unreachable!()
        }
    };
    (
        $kind: expr,

        $(
            $($type: tt)|+ => $body: ident
        )+
    ) => {
        match $kind {
            $(
                $(
                    t if t == $type => { ffi_type!($body) }
                )+
            )+
            _ => unreachable!()
        }
    }
}

pub fn str_to_type(ty: &str) -> *mut ffi_type {
    match_ffi_type!(
        ty,
        "c" => c_schar
        "u" => c_int
        "b" => i8
        "h" => c_short
        "H" => c_ushort
        "i" => c_int
        "I" => c_uint
        "l" => c_long
        "q" => c_longlong
        "L" => c_ulong
        "Q" => c_ulonglong
        "f" => f32
        "d" => f64
        "g" => longdouble
        "?" | "B" => c_uchar
        "z" | "Z" => pointer
        "P" => void
    )
}

fn py_to_ffi(
    ty: *mut *mut ffi_type,
    obj: PyObjectRef,
    vm: &VirtualMachine,
) -> PyResult<*mut c_void> {
    let res = match_ffi_type!(
        unsafe { *ty },
        c_schar => {
            &mut i8::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_int => {
            &mut i32::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_short => {
            &mut i16::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_ushort => {
            &mut u16::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_uint => {
            &mut u32::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        //@ TODO: Convert c*longlong from BigInt?
        c_long | c_longlong => {
            &mut i64::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_ulong | c_ulonglong => {
            &mut u64::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        f32 => {
            &mut f32::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        f64 | longdouble=> {
            &mut f64::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        c_uchar => {
            &mut u8::try_from_object(vm, obj)? as *mut _ as *mut c_void
        }
        pointer => {
            usize::try_from_object(vm, obj).unwrap() as *mut c_void
        }
        void => {
            ptr::null_mut()
        }
    );

    Ok(res)
}

#[derive(Debug)]
pub struct Function {
    pointer: *mut c_void,
    cif: ffi_cif,
    arguments: Vec<*mut ffi_type>,
    return_type: Box<*mut ffi_type>,
}

impl Function {
    pub fn new(fn_ptr: usize, arguments: Vec<String>, return_type: &str) -> Function {
        Function {
            pointer: fn_ptr as *mut _,
            cif: Default::default(),
            arguments: arguments.iter().map(|s| str_to_type(s.as_str())).collect(),

            return_type: Box::new(str_to_type(return_type)),
        }
    }
    pub fn set_args(&mut self, args: Vec<String>) {
        self.arguments.clear();
        self.arguments
            .extend(args.iter().map(|s| str_to_type(s.as_str())));
    }

    pub fn set_ret(&mut self, ret: &str) {
        (*self.return_type.as_mut()) = str_to_type(ret);
    }

    pub fn call(
        &mut self,
        arg_ptrs: Vec<PyObjectRef>,
        vm: &VirtualMachine,
    ) -> PyResult<PyObjectRc> {
        let result = unsafe {
            prep_cif(
                &mut self.cif,
                ABI,
                self.arguments.len(),
                *self.return_type.as_mut(),
                self.arguments.as_mut_ptr(),
            )
        };

        if let Err(FFIError::Typedef) = result {
            return Err(vm.new_runtime_error(
                "The type representation is invalid or unsupported".to_string(),
            ));
        } else if let Err(FFIError::Abi) = result {
            return Err(vm.new_runtime_error("The ABI is invalid or unsupported".to_string()));
        }

        let arg_results: Result<Vec<*mut c_void>, _> = arg_ptrs
            .iter()
            .zip(self.arguments.iter_mut())
            .map(|(o, t)| py_to_ffi(t as *mut *mut _, o.clone(), vm))
            .collect();

        let fun_ptr = CodePtr(self.pointer);

        let cif_ptr = &mut self.cif;
        let mut arg_pointers = arg_results?;

        let res = unsafe {
            match_ffi_type!(
                *self.return_type.as_mut(),
                c_schar => {
                    let r: c_schar = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as i8)
                }
                c_int => {
                    let r: c_int = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as i32)
                }
                c_short => {
                    let r: c_short = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as i16)
                }
                c_ushort => {
                    let r: c_ushort = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as u16)
                }
                c_uint => {
                    let r: c_uint = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as u32)
                }
                c_long | c_longlong => {
                    let r: c_long = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as i64)
                }
                c_ulong | c_ulonglong => {
                    let r: c_ulong = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as u64)
                }
                f32 => {
                    let r: c_float = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as f32)
                }
                f64 | longdouble=> {
                    let r: c_double = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as f64)
                }
                c_uchar => {
                    let r: c_uchar = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as u8)
                }
                pointer => {
                    let r: *mut c_void = ffi_call(cif_ptr, fun_ptr, arg_pointers.as_mut_ptr());
                    vm.new_pyobj(r as *const _ as usize)
                }
                void => {
                    vm.ctx.none()
                }
            )
        };

        Ok(res)
    }
}

unsafe impl Send for Function {}
unsafe impl Sync for Function {}

#[pyclass(module = "_ctypes", name = "CFuncPtr", base = "PyCData")]
pub struct PyCFuncPtr {
    pub _name_: String,
    pub _argtypes_: AtomicCell<Vec<PyObjectRef>>,
    pub _restype_: AtomicCell<PyObjectRef>,
    _handle: PyObjectRc,
    _f: PyRwLock<Function>,
}

impl fmt::Debug for PyCFuncPtr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "PyCFuncPtr {{ _name_, _argtypes_, _restype_}}")
    }
}

impl PyValue for PyCFuncPtr {
    fn class(_vm: &VirtualMachine) -> &PyTypeRef {
        Self::static_type()
    }
}

#[pyimpl(with(Callable), flags(BASETYPE))]
impl PyCFuncPtr {
    #[pyproperty(name = "_argtypes_")]
    fn argtypes(&self, vm: &VirtualMachine) -> PyObjectRef {
        vm.ctx
            .new_list(unsafe { &*self._argtypes_.as_ptr() }.clone())
    }

    #[pyproperty(name = "_restype_")]
    fn restype(&self, _vm: &VirtualMachine) -> PyObjectRef {
        unsafe { &*self._restype_.as_ptr() }.clone()
    }

    #[pyproperty(name = "_argtypes_", setter)]
    fn set_argtypes(&self, argtypes: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        if vm.isinstance(&argtypes, &vm.ctx.types.list_type).is_ok()
            || vm.isinstance(&argtypes, &vm.ctx.types.tuple_type).is_ok()
        {
            let args: Vec<PyObjectRef> = vm.extract_elements(&argtypes)?;
            let c_args_res: PyResult<Vec<PyObjectRef>> = args
                .iter()
                .enumerate()
                .map(|(idx, inner_obj)| {
                    match vm.isinstance(inner_obj, PySimpleType::static_type()) {
                        // @TODO: checks related to _type_ are temporary
                        // it needs to check for from_param method, instead
                        Ok(_) => Ok(vm.get_attribute(inner_obj.clone(), "_type_").unwrap()),
                        _ => Err(vm.new_type_error(format!(
                            "item {} in _argtypes_ must be subclass of _SimpleType, but type {} found",
                            idx,
                            inner_obj.class().name
                        ))),
                    }
                })
                .collect();

            let c_args = c_args_res?;

            self._argtypes_.store(c_args.clone());

            let str_types: Vec<String> = c_args
                .iter()
                .map(|obj| vm.to_str(&obj).unwrap().to_string())
                .collect();

            let mut fn_ptr = self._f.write();
            fn_ptr.set_args(str_types);

            Ok(())
        } else {
            Err(vm.new_type_error(format!(
                "_argtypes_ must be a sequence of types, {} found.",
                argtypes.to_string()
            )))
        }
    }

    #[pyproperty(name = "_restype_", setter)]
    fn set_restype(&self, restype: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        match vm.isinstance(&restype, PySimpleType::static_type()) {
            // @TODO: checks related to _type_ are temporary
            Ok(_) => match vm.get_attribute(restype.clone(), "_type_") {
                Ok(_type_) => {
                    // @TODO: restype must be a type, a callable, or None
                    self._restype_.store(restype.clone());
                    let mut fn_ptr = self._f.write();
                    fn_ptr.set_ret(vm.to_str(&_type_)?.as_ref());

                    Ok(())
                }
                Err(_) => Err(vm.new_attribute_error("atribute _type_ not found".to_string())),
            },

            Err(_) => Err(vm.new_type_error(format!(
                "value is not an instance of _CData, type {} found",
                restype.class().name
            ))),
        }
    }

    // @TODO: Needs to check and implement other forms of new
    #[pyslot]
    fn tp_new(
        cls: PyTypeRef,
        func_name: PyStrRef,
        arg: PyObjectRef,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        match vm.get_attribute(cls.as_object().to_owned(), "_argtypes_") {
            Ok(_) => Self::from_dll(cls, func_name, arg, vm),
            Err(_) => Err(vm.new_type_error(
                "cannot construct instance of this class: no argtypes slot".to_string(),
            )),
        }
    }

    /// Returns a PyCFuncPtr from a Python DLL object
    /// # Arguments
    ///
    /// * `func_name` - A string that names the function symbol
    /// * `arg` - A Python object with _handle attribute of type int
    ///
    fn from_dll(
        cls: PyTypeRef,
        func_name: PyStrRef,
        arg: PyObjectRef,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        if let Ok(h) = vm.get_attribute(arg.clone(), "_handle") {
            if let Ok(handle) = h.downcast::<PyInt>() {
                let handle_obj = handle.clone().into_object();
                let ptr_fn = dlsym(handle, func_name.clone(), vm)?;
                let fn_ptr = usize::try_from_object(vm, ptr_fn)?;

                PyCFuncPtr {
                    _name_: func_name.to_string(),
                    _argtypes_: AtomicCell::default(),
                    _restype_: AtomicCell::new(vm.ctx.none()),
                    _handle: handle_obj,
                    _f: PyRwLock::new(Function::new(
                        fn_ptr,
                        Vec::new(),
                        "i", // put a default here
                    )),
                }
                .into_ref_with_type(vm, cls)
            } else {
                Err(vm.new_type_error(format!("_handle must be an int not {}", arg.class().name)))
            }
        } else {
            Err(vm.new_attribute_error(
                "positional argument 2 must have _handle attribute".to_string(),
            ))
        }
    }
}

impl Callable for PyCFuncPtr {
    fn call(zelf: &PyRef<Self>, args: FuncArgs, vm: &VirtualMachine) -> PyResult {
        let inner_args = unsafe { &*zelf._argtypes_.as_ptr() };

        if args.args.len() != inner_args.len() {
            return Err(vm.new_runtime_error(format!(
                "this function takes at least {} argument{} ({} given)",
                inner_args.len(),
                if !inner_args.is_empty() { "s" } else { "" },
                args.args.len()
            )));
        }

        let arg_res: Result<Vec<PyObjectRef>, _> = args
            .args
            .iter()
            .enumerate()
            .map(|(idx, obj)| {
                if vm
                    .issubclass(&obj.clone_class(), PySimpleType::static_type())
                    .is_ok()
                {
                    Ok(vm.get_attribute(obj.clone(), "value")?)
                } else {
                    Err(vm.new_type_error(format!(
                        "positional argument {} must be subclass of _SimpleType, but type {} found",
                        idx,
                        obj.class().name
                    )))
                }
            })
            .collect();

        (*zelf._f.write()).call(arg_res?, vm)
    }
}
