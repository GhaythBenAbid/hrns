import ctypes, ctypes.util
CG = ctypes.cdll.LoadLibrary(ctypes.util.find_library("CoreGraphics"))
CF = ctypes.cdll.LoadLibrary(ctypes.util.find_library("CoreFoundation"))
vp = ctypes.c_void_p
CG.CGWindowListCopyWindowInfo.restype = vp; CG.CGWindowListCopyWindowInfo.argtypes = [ctypes.c_uint32, ctypes.c_uint32]
CF.CFArrayGetCount.restype = ctypes.c_long; CF.CFArrayGetCount.argtypes = [vp]
CF.CFArrayGetValueAtIndex.restype = vp; CF.CFArrayGetValueAtIndex.argtypes = [vp, ctypes.c_long]
CF.CFDictionaryGetValue.restype = vp; CF.CFDictionaryGetValue.argtypes = [vp, vp]
CF.CFStringGetCString.restype = ctypes.c_bool; CF.CFStringGetCString.argtypes = [vp, ctypes.c_char_p, ctypes.c_long, ctypes.c_uint32]
CF.CFNumberGetValue.restype = ctypes.c_bool; CF.CFNumberGetValue.argtypes = [vp, ctypes.c_int, vp]
CF.CFRelease.argtypes = [vp]
CG.CGRectMakeWithDictionaryRepresentation.restype = ctypes.c_bool
class CGRect(ctypes.Structure): _fields_ = [("x", ctypes.c_double), ("y", ctypes.c_double), ("w", ctypes.c_double), ("h", ctypes.c_double)]
CG.CGRectMakeWithDictionaryRepresentation.argtypes = [vp, ctypes.POINTER(CGRect)]
K = {n: vp.in_dll(CG, n).value for n in ["kCGWindowOwnerName", "kCGWindowOwnerPID", "kCGWindowLayer", "kCGWindowBounds"]}
def _s(r):
    b = ctypes.create_string_buffer(512)
    return b.value.decode() if r and CF.CFStringGetCString(r, b, 512, 0x08000100) else None
def _i(r):
    v = ctypes.c_int64()
    return v.value if r and CF.CFNumberGetValue(r, 4, ctypes.byref(v)) else None
def windows():
    arr = CG.CGWindowListCopyWindowInfo(1, 0)  # on-screen only
    out = []
    for i in range(CF.CFArrayGetCount(arr)):
        d = CF.CFArrayGetValueAtIndex(arr, i)
        r = CGRect(); CG.CGRectMakeWithDictionaryRepresentation(CF.CFDictionaryGetValue(d, K["kCGWindowBounds"]), ctypes.byref(r))
        out.append((_s(CF.CFDictionaryGetValue(d, K["kCGWindowOwnerName"])), _i(CF.CFDictionaryGetValue(d, K["kCGWindowOwnerPID"])),
                    _i(CF.CFDictionaryGetValue(d, K["kCGWindowLayer"])), r.w, r.h))
    CF.CFRelease(arr)
    return out
def has_window(owner):
    return any(o == owner and l == 0 and h > 200 for o, p, l, w, h in windows())
if __name__ == "__main__":
    for w in windows():
        if w[2] == 0: print(w)
