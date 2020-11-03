import ctypes

# ctypes.CDLL("/home/shammyz/Documents/sandbox/libhello.so")
# ctypes.dlopen("/home/shammyz/Documents/sandbox/libhello.so")
lib = ctypes.CDLL("/home/shammyz/Documents/sandbox/libhello.so")
lib.hello()
assert False
