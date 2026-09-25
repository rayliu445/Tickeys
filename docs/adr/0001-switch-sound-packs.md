# ADR 0001：新增机械轴音效包

- 状态：已接受
- 日期：2026-09-25
- 分支：`feat/switch-sound-packs`

## 背景

需求原话：现在的音效有打字机、气泡那几种，想加"轴"的音效 —— 能选各种轴的那种，
**偏办公场景、不要太吵**（不要"打游戏那种特别吵的"）。

调查之后发现四件事，它们直接决定了方案：

1. **仓库里已经有 2 款轴了**：`Cherry_G80_3000` 和 `Cherry_G80_3494`（0.4.0 那版加的）。
   现有音效一共 7 款：`bubble` / `typewriter` / `mechanical` / `sword` / `drum` + 上面两款 Cherry。
2. **这个仓库不是用户实际在跑的程序。** 仓库是 Rust 版 0.5.0（最后一次提交 2020-07-31）；
   用户机器上跑的是 `/Applications/Tickeys.app` 1.1.0 —— C# / Xamarin.Mac 版（MonoBundle + Tickeys.exe），
   构建于 2020-12-03。两者读的是同一套 `schemes.json` 格式。
3. **这个仓库在 Rust 1.89 下编译不过。** `cargo check` 失败在 `objc 0.1.8`、`rustc-serialize`、
   `core-foundation-sys` —— 2015 年的依赖跟现代 rustc 不兼容。要编译得退回 2016 年前后的工具链，
   还要装 `freealut`（Homebrew 早已移除）。
4. **本机没有 Xcode**（只有 CommandLineTools，连 `ibtool` 都没有）→ `Settings.nib` / storyboard
   都改不了 → **任何"给轴加个分类/二级菜单"的 UI 改动都做不到**（而且 1.1.0 是闭源 C#，本机没有源码）。

## 决策

### 1. 只做数据改动，一行代码都不改

加一个音效 = 新增一个目录 + `schemes.json` 里加一条。`src/tickeys.rs` 的 `load_scheme()`
只按 `schemes.json` 里的 `files` / `non_unique_count` / `key_audio_map` 加载，
新音效走的是完全相同的代码路径。所以不需要编译、不需要新依赖、不需要碰 Rust。

### 2. 不做 UI 分类，新音效就是下拉框里多几行

见背景第 4 条 —— 做不到。接受下拉列表变长的代价。

### 3. 选轴标准：同音量下的高频尖刺成分，而不是"轴的名字"

关键指标是 **4kHz 以上的能量相对于整体响度的比值**。
原因是音量条能压"响"，压不掉"尖"：同样把音量调到一样大，有的轴依然又闷又平，有的轴高频扎耳。
只比绝对响度会得出错误结论 —— 有的包只是录得小声而已。

测量方法（可复现）：

```sh
# 先掐掉首尾静音，保证各包可比（否则长尾静音会拉低 RMS）
TRIM="silenceremove=start_periods=1:start_threshold=-50dB:start_silence=0.005,areverse,silenceremove=start_periods=1:start_threshold=-50dB:start_silence=0.005,areverse"
ffmpeg -i in.wav -af "$TRIM,astats=measure_perchannel=none:measure_overall=Peak_level+RMS_level" -f null -
ffmpeg -i in.wav -af "$TRIM,highpass=f=4000,astats=measure_perchannel=none:measure_overall=RMS_level" -f null -
# 尖刺 = 上面第二个 RMS - 第一个 RMS
```

全部候选的实测结果（同一个包取 5~8 个普通键求平均）：

| 音效包 | 来源 | 绝对响度 (RMS dBFS) | 尖刺（绝对） | **同音量下的尖刺** |
|---|---|---|---|---|
| sword | 仓库现有 | -20.2 | -48.0 | -27.9 |
| bubble | 仓库现有 | -28.0 | -51.8 | -23.8 |
| drum | 仓库现有 | -18.0 | -39.0 | -21.0 |
| **Topre 静电容** | 本次新增 | -27.6 | -42.3 | **-14.7** |
| Holy Panda | 未选用 | -30.1 | -39.3 | -9.2 |
| Cherry MX 红轴 | 未选用 | -28.0 | -37.2 | -9.2 |
| **NK Cream** | 本次新增 | -40.7 | -49.3 | **-8.6** |
| **Everglide Oreo** | 本次新增 | -32.7 | -41.0 | **-8.3** |
| turquoise | 未选用 | -31.8 | -38.9 | -7.1 |
| Everglide 紫轴 | 未选用 | -28.5 | -35.7 | -7.1 |
| mechanical | 仓库现有 | -26.4 | -33.4 | -6.9 |
| Cherry G80-3494 | 仓库现有 | -24.7 | -31.5 | -6.8 |
| Cherry MX 黑轴 | 未选用 | -33.6 | -40.2 | -6.6 |
| Cherry G80-3000 | 仓库现有 | -27.2 | -33.8 | -6.6 |
| Cherry MX 茶轴 | 未选用 | -30.0 | -34.7 | -4.8 |
| typewriter | 仓库现有 | -36.3 | -39.3 | -3.0 |
| Cherry MX 蓝轴（青轴） | 未选用 | -25.1 | -27.9 | **-2.7** |

三条结论：

- **"打游戏那种特别吵的" = MX 蓝轴（青轴）**，-2.7 是全表最尖的。
- **MX 茶轴是第二尖的**（-4.8）—— 它的段落"咔哒"在高频上很明显，跟"办公安静"的直觉相反。
- **仓库现有的两个 Cherry 包（-6.8 / -6.6）比所有新增候选都尖。** 也就是说"办公向"的音效
  在现有 7 款里其实不存在。

### 4. 选定三款

| 目录名 | 下拉框显示名（中文） | 选它的理由 |
|---|---|---|
| `Topre` | Topre 静电容 | 全表最闷，比第二名低 5.5dB；静电容的闷响和机械是两个味道 |
| `NK_Cream` | NK Cream 奶油轴 | 绝对响度最低；上游提供 35 个独立按键 WAV，**没有经过切分**，最干净 |
| `Everglide_Oreo` | Everglide Oreo 奥利奥轴 | 闷度接近 NK Cream，第三个味道 |

淘汰理由：MX 蓝（青轴）最吵；MX 茶第二吵；MX 红/MX 黑 用户实际听过之后认为偏吵
（实测 MX 黑 -6.6 确实在偏尖那档，用户听感与数据一致）。

> 注意：**轴的名字不等于声音。** 同一个型号的轴，不同录音可以从很闷到很尖。
> 所以最终判断必须基于"实际要装进去的那几个文件"，而不是型号名。

### 5. 素材取自 Mechvibes（MIT），不用 Freesound

现有音效包都来自 Freesound，且 `bubble` / `mechanical` / `drum` 的 `license.txt` 里列的是
**CC-BY-NC** —— 跟本仓库的 MIT 协议是冲突的（历史遗留，本次不处理）。

新素材取自：

- `https://github.com/hainguyents13/mechvibes`（MIT, Copyright (c) 2021 Hai Nguyen）
- `https://github.com/diogo7dias/omaclack`（MIT）—— 它把 Mechvibes 官方那两个
  "一整段打字录音"的包切成了单键文件

两者都是 MIT，与本仓库一致。每个新音效目录下都放了 `license.txt` 记录来源与处理过程。

### 6. 每个音效 8 个普通键 + 真实的空格/回车/退格

```
files:           1.wav .. 8.wav, space.wav, enter.wav, backspace.wav
non_unique_count: 8                              -> 普通键走 keycode % 8
key_audio_map:   {"36": 9, "49": 8, "51": 10}    -> 回车 / 空格 / 退格
```

- 8 个普通键是为了降低重复感（现有 `sword` 只有 6 个、`mechanical` 只有 4 个）。
- 空格/回车/退格用的是**源素材里真实对应按键的录音**，不是随便指一个索引 ——
  比现有 `key_audio_map` 里随意填的索引质量高一档。
- macOS 键码：`36` = Return，`49` = Space，`51` = Backspace。
  Mechvibes 的文件名是 Windows 虚拟键码：`13` = Enter，`32` = Space，`8` = Backspace。

### 7. 逐文件响度对齐到 -25dBFS，而不是整包一个增益

**这一条推翻了最初的方案。** 原计划"整包一个统一增益、保留包内自然音量差"，
实测发现源素材里**各个键本身的音量差最多 22dB**：

| 包 | 退格 | 回车 | 包内峰值差 |
|---|---|---|---|
| Everglide Oreo | -0.50 dBFS | -22.41 dBFS | 21.9 dB |
| NK Cream | -0.50 dBFS | -12.29 dBFS | 11.8 dB |

按原方案做会退格爆音、回车几乎听不见。所以改成：

```
每个文件的增益 = min(-25.0 - 该文件RMS, -0.5 - 该文件峰值)
```

即对齐到 -25dBFS RMS，同时留 0.5dB 的峰值余量防削波。三个包都对齐到同一水平，
所以可以在音量条不动的情况下公平 A/B。

### 8. 剔除静音坏样本

`topre` 是社区从一整段录音里切出来的，其中 **VK 66（B 键）那个文件完全静音**
（峰值 -62.7dB，其他键都在 -5 ~ -17dB）。照搬会有 1/8 的按键不出声。
所以流程里加了静音检测（峰值 < -40dBFS 即剔除）。

上游 `nk-cream` 提供的就是 35 个独立按键 WAV，不存在这个问题。

### 9. 目录名必须纯 ASCII

`src/tickeys.rs` 的 `load_scheme()` 里：

```rust
let base_path_len = path.chars().count();   // 字符数
...
path.truncate(base_path_len);               // truncate 要的是字节数
```

`String::truncate` 收的是**字节下标**，这里却传了**字符数**。路径全 ASCII 时两者相等，
一旦路径里出现中文（UTF-8 下 1 字符 = 3 字节）就会截在错误的字节位置，路径损坏 → `panic!`。
而目录名会被直接拼进路径（`settings_ui.rs`: `"data/".to_string() + &sch.name`）。

**所以目录名只能是 ASCII**（`Topre` / `NK_Cream` / `Everglide_Oreo`），中文只放在下拉框显示名里。
这是绕开这个 bug，不是修它。

### 10. 必须补 `Localizable.strings` 的键

`src/cocoa_util.rs`：

```rust
msg_send![bundle, localizedStringForKey: key value: @"" table: nil]
```

`value:` 传的是空串，意味着**键找不到就返回空字符串**。而下拉框显示名要走这个查找
（`settings_ui.rs`: `addItemWithTitle: l10n_str(&s.display_name)`）。

查过全部 git 历史：`"Cherry G80-3000"` / `"Cherry G80-3494"` 这两个键**从来没有被添加过**，
所以这两条在 Rust 版下拉框里一直是**空白行**。新音效如果不补键，会重复同样的错误。

本次补了 5 个键，并在两个文件里都补：

- `Base.lproj/Localizable.strings`（UTF-8）
- `zh-Hans.lproj/Localizable.strings`（**UTF-16LE 带 BOM**，必须按原编码写回）

### 11. 只落仓库，不动 `/Applications/Tickeys.app`

用户明确选择。改动不会影响他当前正在跑的 1.1.0。

## 后果

### 已完成

- 新增 3 个音效目录（每个 11 个 wav + `license.txt`，共 228KB）
- `schemes.json` 从 7 条增至 10 条
- 补 5 个本地化键 × 2 个语言文件（含修复两条历史空白行）
- `changelog.md` 增加"未发布"章节

### 已知但**没有**处理的问题

按"只做要求范围内的事"的原则，以下都只是记录下来：

1. **"Drum" 在 `Base.lproj` 里缺键** → 英文界面下 `Drum` 也是空白行（`zh-Hans` 里有）。
   这和 Cherry 那两条是同一类 bug，但不在本次批准范围内。
2. **根因没修。** `l10n_str` 用 `value:@""` 才是空白行的根源。本次只是把键补齐，
   代码仍然"找不到键就返回空串"，将来再加音效还是会踩。
3. **`Info.plist` 里 `CFBundleDevelopmentRegion = "English"`** —— 不是合法的语言 ID（通常是 `en`）。
   在英文环境下 `Base.lproj` 到底会不会被当兜底用，没有验证过（本机编译不了，无法实测）。
4. **现有两个 Cherry 包是全仓库最尖的**（-6.8 / -6.6），没有动它们。
5. **`bubble` / `mechanical` / `drum` 的 CC-BY-NC 授权与仓库 MIT 冲突**，没有动。
6. **`load_scheme` 里 `chars().count()` / `truncate` 的字节-字符混淆 bug** 没有修。
   目录名是 ASCII 时不会触发，但只要 `.app` 本身放在带中文的路径下（例如 `~/桌面/Tickeys.app`）就会炸。
7. **Rust 版整体编译不过**，所以以上所有改动都没有在真实运行中验证过。
   验证手段只有：JSON 可解析、文件存在、键索引在范围内、wav 头是规范的 `RIFF/fmt/data`。

### 验证做了哪些

- `schemes.json` 全 10 条解析通过；每个 `files` 里的文件都存在；
  `key_audio_map` 的索引不越界；`non_unique_count` ≤ 文件数
- 新音效的 `36/49/51` 分别指向 `enter.wav` / `space.wav` / `backspace.wav`
- 33 个 wav 全部是 **单声道 / 44100Hz / 16 位 PCM**，RIFF 里只有 `fmt` + `data`
  （无 `LIST`/`INFO` 等额外 chunk —— 老的 `alut` 解码器对此挑食）
- 每个文件对齐到 -25.00dBFS，峰值最大 -1.04dBFS（不削波）
- `zh-Hans` 文件：原 1090 字节是新文件 1436 字节的**完整前缀**（纯追加），编码仍是 UTF-16LE + BOM，无 CRLF
- `Base` 文件：diff 恰好 +5 行
- 三个目录名经 `LC_ALL=C grep '[^ -~]'` 确认纯 ASCII

### 试听材料

选轴之前做过一个试听包（`~/Desktop/Tickeys-轴音效-试听/`），
每个候选一条"模拟打字"demo + 一条"单键逐个听" + 11 个原始采样，三个候选响度对齐，
让用户先听再定 —— 因为"轴的名字"判断不了听感，而本机没法编译运行来试听。
用户听完后确认三款全要。
