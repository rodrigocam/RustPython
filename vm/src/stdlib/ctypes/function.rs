extern crate libffi;

use ::std::sync::Arc;

use crate::builtins::pystr::PyStrRef;
use crate::builtins::PyTypeRef;
use crate::common::lock::PyRwLock;
use crate::function::FuncArgs;
use crate::pyobject::{
    PyObjectRc, PyObjectRef, PyRef, PyResult, PyValue, StaticType, TypeProtocol,
};
use crate::VirtualMachine;

use crate::stdlib::ctypes::common::{
    convert_type, CDataObject, FunctionProxy, FUNCTIONS, SIMPLE_TYPE_CHARS,
};

use crate::slots::Callable;
use crate::stdlib::ctypes::dll::{dlsym, SharedLibrary};

#[pyclass(module = "_ctypes", name = "CFuncPtr", base = "CDataObject")]
#[derive(Debug)]
pub struct PyCFuncPtr {
    _name_: String,
    _argtypes_: PyRwLock<Vec<PyStrRef>>,
    _restype_: PyRwLock<Option<PyStrRef>>,
    _callable_: Arc<FunctionProxy>,
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
        vm.ctx.new_list(
            self._argtypes_
                .read()
                .iter()
                .map(|a| a.clone().into_object())
                .collect(),
        )
    }

    #[pyproperty(name = "_restype_")]
    fn restype(&self, vm: &VirtualMachine) -> PyObjectRef {
        match &(*self._restype_.read()) {
            Some(restype) => restype.clone().into_object(),
            None => vm.ctx.none(),
        }
        // vm.ctx.new_str(self._restype_)
    }

    #[pyproperty(name = "_argtypes_", setter)]
    fn set_argtypes(&self, argtypes: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        if vm.isinstance(&argtypes, &vm.ctx.types.list_type).is_ok()
            || vm.isinstance(&argtypes, &vm.ctx.types.tuple_type).is_ok()
        {
            let args: Vec<PyObjectRef> = vm.extract_elements(&argtypes).unwrap();

            let c_args: Result<Vec<PyObjectRc>, _> = args
                .iter()
                .enumerate()
                .map(|(idx, inner_obj)| {
                    match vm.isinstance(inner_obj, CDataObject::static_type()) {
                        Ok(_) => match vm.get_attribute(inner_obj.clone(), "_type_") {
                            Ok(_type_)
                                if SIMPLE_TYPE_CHARS.contains(_type_.to_string().as_str()) =>
                            {
                                Ok(_type_)
                            }
                            Ok(_type_) => {
                                Err(vm.new_attribute_error("invalid _type_ value".to_string()))
                            }
                            Err(_) => {
                                Err(vm.new_attribute_error("atribute _type_ not found".to_string()))
                            }
                        },
                        Err(_) => Err(vm.new_type_error(format!(
                            "object at {} is not an instance of _CDataObject, type {} found",
                            idx,
                            inner_obj.class()
                        ))),
                    }
                })
                .collect();

            self._argtypes_.write().clear();
            self._argtypes_.write()
                .extend(c_args?.iter().filter_map(|obj| obj.clone().downcast().ok()));

            // Ok(vm.ctx.none())
            Ok(())
        } else {
            Err(vm.new_type_error(format!(
                "argtypes must be Tuple or List, {} found.",
                argtypes.class()
            )))
        }
    }

    #[pyproperty(name = "_restype_", setter)]
    fn set_restype(&self, restype: PyObjectRef, vm: &VirtualMachine) -> PyResult<()> {
        match vm.isinstance(&restype, CDataObject::static_type()) {
            Ok(_) => match vm.get_attribute(restype, "_type_") {
                Ok(_type_) if SIMPLE_TYPE_CHARS.contains(_type_.to_string().as_str()) => {
                    *self._restype_.write() = _type_.downcast().ok();
                    Ok(())
                }
                Ok(_type_) => Err(vm.new_attribute_error("invalid _type_ value".to_string())),
                Err(_) => Err(vm.new_attribute_error("atribute _type_ not found".to_string())),
            },
            Err(_) => Err(vm.new_type_error(format!(
                "value is not an instance of _CDataObject, type {} found",
                restype.class()
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
                "cannot construct instance of this class: no argtypes".to_string(),
            )),
        }
    }

    /// Returns a PyCFuncPtr from a Python DLL object
    /// # Arguments
    ///
    /// * `func_name` - A string that names the function symbol
    /// * `dll` - A Python object with _handle attribute of type SharedLibrary
    ///
    fn from_dll(
        cls: PyTypeRef,
        func_name: PyStrRef,
        arg: PyObjectRef,
        vm: &VirtualMachine,
    ) -> PyResult<PyRef<Self>> {
        if let Ok(h) = vm.get_attribute(arg.clone(), "_handle") {
            if let Some(handle) = h.payload::<SharedLibrary>() {
                PyCFuncPtr {
                    _name_: func_name.as_ref().to_string(),
                    _argtypes_: PyRwLock::new(Vec::new()),
                    _restype_: PyRwLock::new(None),
                    _callable_: FUNCTIONS.write().get_or_insert_fn(
                        func_name.as_ref(),
                        handle.get_name().as_str(),
                        handle.get_lib(),
                        vm,
                    )?,
                }
                .into_ref_with_type(vm, cls)
            } else {
                Err(vm.new_type_error(format!(
                    "_handle must be SharedLibrary not {}",
                    arg.class().name
                )))
            }
        } else {
            Err(vm.new_attribute_error(
                "positional argument 2 must have _handle attribute".to_string(),
            ))
        }
    }
}

impl Callable for PyCFuncPtr {
    // @TODO: Build args e result before calling.
    fn call(zelf: &PyRef<Self>, args: FuncArgs, vm: &VirtualMachine) -> PyResult {
        if args.args.len() != (*zelf._argtypes_.read()).len() {
            return Err(vm.new_runtime_error(format!(
                "invalid number of arguments, required {}, but {} found",
                (*zelf._argtypes_.read()).len(),
                args.args.len()
            )));
        }

        // Needs to check their types and convert to middle::Arg based on zelf._argtypes_
        // Something similar to the set of _argtypes_
        // arg_vec = ...

        // This is not optimal, but I can't simply store a vector of middle::Type inside PyCFuncPtr
        let c_args = zelf
            ._argtypes_
            .read()
            .iter()
            .map(|str_type| convert_type(str_type.to_string().as_str()))
            .collect();

        let arg_vec = Vec::new();

        let ptr_fn = dlsym(zelf._callable_.get_lib(), zelf._callable_.get_name()).ok();
        let ret = zelf
            ._callable_
            .call(c_args, (*zelf._restype_.read()).as_ref(), arg_vec, ptr_fn, vm);

        // Needs to convert ret back to an object

        Ok(vm.new_pyobj(ret))
    }
}
