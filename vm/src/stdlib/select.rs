use super::os;
use crate::function::OptionalOption;
use crate::obj::objint::PyInt;
use crate::pyobject::{PyObjectRef, PyResult, TryFromObject};
use crate::vm::VirtualMachine;

type RawFd = i64;

struct Selectable {
    fno: RawFd,
}

impl TryFromObject for Selectable {
    fn try_from_object(vm: &VirtualMachine, obj: PyObjectRef) -> PyResult<Self> {
        let fno = RawFd::try_from_object(vm, obj.clone()).or_else(|_| {
            let meth = vm.get_method_or_type_error(obj, "fileno", || {
                "select arg must be an int or object with a fileno() method".to_string()
            })?;
            RawFd::try_from_object(vm, vm.invoke(&meth, vec![])?)
        })?;
        Selectable { fno }
    }
}

#[repr(C)]
pub struct FdSet(libc::fd_set);

impl FdSet {
    pub fn new() -> FdSet {
        let mut fdset = unsafe { mem::uninitialized() };
        unsafe { libc::FD_ZERO(&mut fdset) };
        FdSet(fdset)
    }

    pub fn insert(&mut self, fd: RawFd) {
        unsafe { libc::FD_SET(fd, &mut self.0) };
    }

    pub fn remove(&mut self, fd: RawFd) {
        unsafe { libc::FD_CLR(fd, &mut self.0) };
    }

    pub fn contains(&mut self, fd: RawFd) -> bool {
        unsafe { libc::FD_ISSET(fd, &mut self.0) }
    }

    pub fn clear(&mut self) {
        unsafe { libc::FD_ZERO(&mut self.0) };
    }

    pub fn highest(&mut self) -> Option<RawFd> {
        for i in (0..FD_SETSIZE).rev() {
            let i = i as RawFd;
            if unsafe { libc::FD_ISSET(i, self as *mut _ as *mut libc::fd_set) } {
                return Some(i);
            }
        }

        None
    }
}

fn sec_to_timeval(sec: f64) -> libc::timeval {
    libc::timeval {
        tv_sec: sec.trunc() as _,
        tv_usec: (sec.fract() * 1e6) as _,
    }
}

fn select_select(
    rlist: PyObjectRef,
    wlist: PyObjectRef,
    xlist: PyObjectRef,
    timeout: OptionalOption<f64>,
    vm: &VirtualMachine,
) -> PyResult<PyObjectRef> {
    use nix::sys::select;
    use nix::sys::time::{TimeVal, TimeValLike};
    use std::os::unix::io::RawFd;

    let seq2set = |list| -> PyResult<(Vec<i32>, FdSet)> {
        let v = vm.extract_elements::<Selectable>(list)?;
        let mut fds = FdSet::new();
        for fd in &v {
            fds.insert(*fd);
        }
        Ok((v, fds))
    };

    let (rlist, mut r) = seq2set(&rlist)?;
    let (wlist, mut w) = seq2set(&wlist)?;
    let (xlist, mut x) = seq2set(&xlist)?;

    let nfds = [&mut r, &mut w, &mut x]
        .iter_mut()
        .filter_map(|set| set.highest())
        .max()
        .unwrap_or(-1)
        + 1;

    let mut timeout = timeout.flat_option().map(sec_to_timeval);
    let timeout = match timeout {
        Some(ref mut tv) => tv as *mut _,
        None => std::ptr::null_mut(),
    };

    unsafe { libc::select(nfds, &mut r, &mut w, &mut x, timeout) };
    // .map_err(|err| super::os::convert_nix_error(vm, err))?;

    let set2list = |list: Vec<RawFd>, mut set: FdSet| -> PyObjectRef {
        vm.ctx.new_list(
            list.into_iter()
                .filter(|fd| set.contains(*fd))
                .map(|fd| vm.new_int(fd))
                .collect(),
        )
    };

    let rlist = set2list(rlist, r);
    let wlist = set2list(wlist, w);
    let xlist = set2list(xlist, x);

    Ok(vm.ctx.new_tuple(vec![rlist, wlist, xlist]))
}

pub fn make_module(vm: &VirtualMachine) -> PyObjectRef {
    py_module!(vm, "select", {
        "select" => vm.ctx.new_rustfunc(select_select),
    })
}
