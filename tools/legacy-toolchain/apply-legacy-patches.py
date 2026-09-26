#!/usr/bin/env python3
"""Make Tickeys 0.5.0 (2015) build with rustc 1.19.0 on macOS 15.

Three ancient crates contain constructs that later rustc versions reject. Each fix
below is a source-level workaround for a diagnostic that did not exist when the
crate was written. The crates are patched *in cargo's extracted registry sources*,
so this script must run AFTER the first `cargo fetch` / failed `cargo build`, and be
re-run whenever cargo re-extracts them (e.g. after `cargo clean` of the registry).

Usage:
    python3 apply-legacy-patches.py                 # patch CARGO_HOME default
    python3 apply-legacy-patches.py /path/to/registry/src/<index-dir>

Every patch is idempotent: re-running is safe.

  1. block 0.1.4            E0591  transmute of a zero-sized fn item
  2. mach 0.0.3             E0517  #[repr(C)] on a type alias
  3. objc 0.1.8             E0591  transmute of zero-sized extern fn declarations
"""
import glob
import os
import re
import sys

# ---------------------------------------------------------------- block 0.1.4
BLOCK_OLD = """                unsafe {
                    ConcreteBlock::with_invoke(
                        mem::transmute($f::<$($t,)* R, X>), self)
                }"""

BLOCK_NEW = """                unsafe {
                    // patched: `$f::<...>` is a zero-sized fn *item*; rustc rejects
                    // transmuting a zero-sized type (E0591). Coerce it to a concrete
                    // fn pointer first, then transmute pointer -> variadic fn
                    // pointer, which is a same-size pointer cast.
                    let invoke: unsafe extern fn(
                            *mut ConcreteBlock<($($t,)*), R, X> $(, $t)*) -> R =
                        $f::<$($t,)* R, X>;
                    ConcreteBlock::with_invoke(
                        mem::transmute(invoke), self)
                }"""

# ----------------------------------------------------------------- mach 0.0.3
MACH_OLD = """#[repr(C)]
pub type mach_port_name_t = natural_t;

#[repr(C)]
struct ipc_port;

#[repr(C)]
pub type ipc_port_t = *mut ipc_port;"""

MACH_NEW = """pub type mach_port_name_t = natural_t;

#[repr(C)]
struct ipc_port;

pub type ipc_port_t = *mut ipc_port;"""

# ----------------------------------------------------------------- objc 0.1.8
OBJC_CONSTS = '''
// patched: these are `extern` *declarations*, i.e. zero-sized fn items, and rustc
// refuses `mem::transmute(zero_sized_fn_item)` (E0591). Binding each to a `const`
// of the matching function-pointer type makes it a real pointer, which transmutes
// fine. The transmute sites below are rewritten to use these consts.
const MSG_SEND: unsafe extern fn(*mut runtime::Object, runtime::Sel, ...)
    -> *mut runtime::Object = runtime::objc_msgSend;
#[cfg(target_arch = "x86")]
const MSG_SEND_FPRET: unsafe extern fn(*mut runtime::Object, runtime::Sel, ...)
    -> f64 = runtime::objc_msgSend_fpret;
#[cfg(not(target_arch = "aarch64"))]
const MSG_SEND_STRET: unsafe extern fn(*mut runtime::Object, runtime::Sel, ...)
    = runtime::objc_msgSend_stret;
const MSG_SEND_SUPER: unsafe extern fn(*const runtime::Super, runtime::Sel, ...)
    -> *mut runtime::Object = runtime::objc_msgSendSuper;
#[cfg(not(target_arch = "aarch64"))]
const MSG_SEND_SUPER_STRET: unsafe extern fn(*const runtime::Super, runtime::Sel, ...)
    = runtime::objc_msgSendSuper_stret;
'''

OBJC_ALIASES = [
    ("objc_msgSend_fpret", "MSG_SEND_FPRET"),
    ("objc_msgSend_stret", "MSG_SEND_STRET"),
    ("objc_msgSendSuper_stret", "MSG_SEND_SUPER_STRET"),
    ("objc_msgSendSuper", "MSG_SEND_SUPER"),
    ("objc_msgSend", "MSG_SEND"),
]


def patch_block(root):
    hits = []
    for path in glob.glob(f"{root}/block-0.1.4/src/lib.rs"):
        s = open(path).read()
        if BLOCK_NEW in s:
            hits.append((path, "已打过"))
        elif BLOCK_OLD in s:
            open(path, "w").write(s.replace(BLOCK_OLD, BLOCK_NEW, 1))
            hits.append((path, "已打补丁"))
        else:
            hits.append((path, "!! 未匹配"))
    return hits


def patch_mach(root):
    hits = []
    for path in glob.glob(f"{root}/mach-0.0.3/src/port.rs"):
        s = open(path).read()
        if MACH_NEW in s:
            hits.append((path, "已打过"))
        elif MACH_OLD in s:
            open(path, "w").write(s.replace(MACH_OLD, MACH_NEW, 1))
            hits.append((path, "已打补丁"))
        else:
            hits.append((path, "!! 未匹配"))
    return hits


def patch_objc(root):
    hits = []
    for path in glob.glob(f"{root}/objc-0.1.8/src/message.rs"):
        s = open(path).read()
        if "MSG_SEND_FPRET" in s:
            hits.append((path, "已打过"))
            continue
        # 插入 const 定义：放在最后一个 use 之后
        lines = s.split("\n")
        last_use = max(i for i, l in enumerate(lines) if l.startswith("use "))
        s = "\n".join(lines[:last_use + 1]) + "\n" + OBJC_CONSTS + "\n".join(lines[last_use + 1:])
        n = 0
        for name, const in OBJC_ALIASES:          # 长名优先，避免前缀误伤
            s, k = re.subn(r"mem::transmute\(runtime::" + name + r"\)",
                           "mem::transmute(" + const + ")", s)
            n += k
        open(path, "w").write(s)
        hits.append((path, f"已打补丁（{n} 处 transmute）"))
    return hits


def default_roots():
    home = os.environ.get("CARGO_HOME", os.path.expanduser("~/.cargo"))
    return sorted(glob.glob(os.path.join(home, "registry", "src", "*")))


def main(argv):
    roots = argv[1:] or default_roots()
    if not roots:
        print("找不到 registry 源目录：先跑一次 cargo fetch / cargo build 再执行本脚本")
        return 1
    total = 0
    for root in roots:
        print(f"registry: {root}")
        for name, fn in (("block", patch_block), ("mach", patch_mach), ("objc", patch_objc)):
            hits = fn(root)
            if not hits:
                print(f"  {name:<6} 未找到（可能还没下载）")
            for path, status in hits:
                print(f"  {name:<6} {status}: {os.path.basename(os.path.dirname(os.path.dirname(path)))}")
                total += 1
    return 0 if total else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
