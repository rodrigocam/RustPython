import ctypes

# ctypes.CDLL("/home/shammyz/Documents/sandbox/libhello.so")
# ctypes.dlopen("/home/shammyz/Documents/sandbox/libhello.so")
lib = ctypes.cdll("/home/shammyz/Documents/sandbox/libhello.so")
lib.hello()
lib.ola()
assert False
