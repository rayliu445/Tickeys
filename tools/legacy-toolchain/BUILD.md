# 在 macOS 15 上编译 Tickeys 0.5.0

2015 年的代码 + 2015 年的依赖，在今天的机器上编译需要绕三个坑。这份文档记录的是**已经实测跑通**的配方。

产物：`target/release/Tickeys`，Mach-O 64 位 **x86_64**，约 1.78MB，在 Apple Silicon 上通过 Rosetta 运行。

---

## 为什么这么麻烦

一句话：**工具链不能升级，只能降级。**

`objc 0.1.8`（2015 年）在 rustc ≥ 1.25 上直接编译错误：

```
error[E0591]: can't transmute zero-sized type
```

这不是"换个新版本就好"的问题 —— 是硬错误，任何现代工具链都过不去。想用现代工具链，必须把 `objc`/`cocoa` 换成 `objc2`/`objc2-app-kit`，那等于重写所有调 Objective-C 的代码（10 个源文件里的 8 个）。

所以只能：**钉一个老工具链** + 把几个依赖里当年合法、现在非法的写法补上。

---

## 需要什么

| 项目 | 版本 | 说明 |
|---|---|---|
| rustc / cargo | **1.19.0**，`x86_64-apple-darwin` | 不能用 Homebrew 的 rust（那是 arm64 的现代版本）。1.25 也不行：`rustc-serialize` 和 `core-foundation-sys` 会在 1.25 上报错 |
| Xcode CLT | 任意 | 只用到系统框架 |
| 系统 | macOS 15 实测 | — |

工具链下载（约 120MB，装到任意目录，不污染系统）：

```sh
curl -fLO https://static.rust-lang.org/dist/2017-07-20/rust-1.19.0-x86_64-apple-darwin.tar.gz
tar xzf rust-1.19.0-x86_64-apple-darwin.tar.gz
cd rust-1.19.0-x86_64-apple-darwin
./install.sh --prefix="$PWD/../../rust-119" --disable-ldconfig
```

> 这是 x86_64 的编译器，在 Apple Silicon 上靠 Rosetta 运行 —— 需要已安装 Rosetta。

---

## 三个坑

### 坑 1：链接器不认老 rlib

rustc 1.19 的 rlib 压缩包里除了目标文件，还有 `rust.metadata.bin` 和 `*.bc.z`。macOS 15 的 `ld` 直接拒绝：

```
ld: archive member 'rust.metadata.bin' not a mach-o file in '.../libstd-....rlib'
```

连 `-Wl,-ld_classic` 也不行。**链接阶段只需要 `.o`**（元数据在编译阶段已经读过了），所以 `wrap-ld.sh` 在链接前把 rlib 复制一份、重建为只含 `.o` 的压缩包：

```sh
export RUSTFLAGS="-C linker=/path/to/wrap-ld.sh"
```

### 坑 2：三个老依赖的写法现在是非法的

`apply-legacy-patches.py` 就地修 cargo 解压出来的 registry 源码（幂等，可重复跑）：

| crate | 错误 | 修法 |
|---|---|---|
| `block 0.1.4` | E0591 | 把零大小的函数项先 `as` 成具体函数指针，再 transmute 指针 |
| `mach 0.0.3` | E0517 | 去掉两个类型别名上多余的 `#[repr(C)]` |
| `objc 0.1.8` | E0591 × 15 | 把 5 个 `extern` 声明绑成 `const` 函数指针，再 transmute |

**必须在第一次 `cargo build`（或 `cargo fetch`）之后跑**，因为要改的是解压出来的源码。cargo 重新解压后要再跑一次。

### 坑 3：`hyper 0.7.2` 默认带 TLS，会拖进 OpenSSL

现代 macOS 没有 OpenSSL 头文件，`openssl 0.7.5` 的构建脚本必失败。而检查更新用的 URL 本来就是 `http://`，不需要 TLS：

```toml
hyper = { version = "0.7.2", default-features = false }
```

---

## 完整步骤

```sh
# 0. 准备：把 toolchain 装到 $TC，把 wrap-ld.sh 放在方便的位置
TC=/path/to/rust-119
export PATH="$TC/bin:$PATH"
export CARGO_HOME="$TC/cargo"
export RUSTFLAGS="-C linker=/path/to/wrap-ld.sh"

# 1. 第一次构建：会失败，但会把依赖拉下来
#    注意：cargo 0.20 用的是 crates.io 的 git 索引，首次会 clone 大约 800MB
cargo build --release || true

# 2. 给三个老依赖打补丁
python3 apply-legacy-patches.py

# 3. 重新构建
cargo build --release

# 4. 组装 app
mkdir -p Tickeys.app/Contents/MacOS
cp target/release/Tickeys Tickeys.app/Contents/MacOS/
rm -rf Tickeys.app/Contents/SharedSupport
cp -r SharedSupport Tickeys.app/Contents/
```

---

## 已知限制

- **只能出 x86_64**。老工具链没有 `aarch64-apple-darwin`。二进制在 Apple Silicon 上走 Rosetta，和仓库里那份 `SharedSupport/libalut.0.dylib`（也是 x86_64）一致。想原生 arm64 + 去掉这个 dylib，只能走代码现代化那条路。
- **产物没有签名**。自己机器上要右键打开；辅助功能权限需要重新授权。要分发得有自己的 Developer ID 证书。
- **构建环境很重**：工具链约 120MB，crates.io git 索引约 800MB。
- **这套配方很脆**：依赖一个 2017 年的编译器、一个链接器包装脚本、三个被打补丁的第三方 crate。它能用，但这不是"修好了"，只是"能编出来了"。真正的修法是把手写 FFI 现代化。
