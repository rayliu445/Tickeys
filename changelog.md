未发布
1.0.0 —— 现代化重写：Apple Silicon 原生 + 可交付版本

实测修复（本机验收通过）
- 修复 macOS 15 上音效加载必崩：Apple 移除了 AVAudioPlayer 的
  initWithContentsOfFile:error:（ObjC 层），改用 initWithData:error:
  先把 wav 读入内存再初始化（附带降低首次播放延迟）
- 修复设置窗口控件全部失灵：重写时漏掉了 NIB 回填 outlet 所需的
  setPopup_audio_scheme: 等 5 个 setter，导致下拉框停留在 NIB 默认的
  "Item 1/2/3" 占位项、音量条无效
- 修复菜单栏图标/黑名单数组"自动释放后悬垂"：arrayWithCapacity: 等
  便利方法返回池子持有的对象，在 app_run 前的 autorelease pool 排空时
  被释放，随后访问即崩。改为 alloc/init 持有 + 显式 retain
- 签名方案落定为专用钥匙串（tickeys-signing）：证书+私钥+分区权限
  全部脚本化配置，codesign 不再弹任何确认框；build.sh 按证书哈希
  精确签名，避免同名证书歧义
- 实测验收：构建 → 安装 → 授权一次 → 再次构建替换 → 启动
  无需重新授权，直接可用

架构
- 整体重写到现代 Rust（2021 edition）+ objc2 0.6：替换 2015 年的
  objc 0.1.8 / cocoa 0.2 / rustc-serialize / openal-rs 等依赖，
  不再需要 2017 年的 Rust 1.19 老工具链和链接器补丁
  （tools/legacy-toolchain 已废弃，仅留作历史参考）
- 二进制为编译机的原生架构（Apple Silicon 上是 arm64 原生，不再走 Rosetta）
- 音频后端从 freealut/OpenAL 换成系统自带的 AVFoundation（AVAudioPlayer）：
  freealut 早已从 Homebrew 移除，捆绑的 dylib 也是 x86_64-only；
  音量条改走 AVAudioPlayer 的 volume 属性（修复音量条拖动无效的问题）
- 键盘钩子改为手写的最小 CoreGraphics FFI（仍为 kCGSessionEventTap 监听模式）
- 睡眠唤醒后钩子失效的自修复改用 NSWorkspace 通知（去掉 IOKit 依赖）
- 移除：检查更新（hyper/block 依赖、2015 年的上游接口已死）、
  NSUserNotification 通知（系统已废弃；可见性由菜单栏图标 + 启动时
  的设置窗口承担）
- 系统设置里找不到本地化键时回退显示键名本身，修复下拉框空白行这类
  问题（旧代码 value: 传空串，任何缺键都渲染成空行）

打包与签名
- bundle id 定为 com.tickeys.Tickeys，版本 1.0.0
- 新增 scripts/setup-signing.sh：创建本地自签代码签名证书 "Tickeys Local"。
  每次构建用同一张证书签名，辅助功能授权与签名绑定，
  因此只需授权一次，之后替换/升级 app 不再反复要求重新授权
  （旧版 ad-hoc 签名每次构建都变，授权跟着失效）
- 新增 scripts/build.sh：编译 → 组装 .app → 写版本号 → 签名（--install 直装）

0.5.0 之后的未发布内容（0.5.x 分支时期）
增加3款机械轴音效：Topre静电容、NK Cream、Everglide Oreo（办公向，均为无段落咔哒的安静轴）
增加：菜单栏（状态栏）图标，带"打开设置"和"退出"菜单。原版是纯后台程序，
      启动后没有任何可见反馈，用户判断不出它在不在运行
修正 Cherry G80-3000 / Cherry G80-3494 在下拉列表中显示为空白的问题
修正：辅助功能权限弹窗会无限重复弹出，用户没有机会去系统设置里勾选
修正：在权限弹窗上点“退出”会导致崩溃（把未初始化的指针当对象析构）
修正：加载音效时读了已释放的内存，导致启动必崩。CString 被当成临时值提前析构，
      as_ptr() 返回野指针，alut 拿到乱码路径报 ALUT_ERROR_IO_ERROR(526)，
      在加载第一个音效文件时就 panic。表现是"双击后没反应"
修正：新增的3款音效被处理流程剪成了11~40毫秒的毛刺，几乎听不见。
      原因是"掐首尾静音"用的 areverse+silenceremove+areverse 把衰减的尾巴
      当成静音剪掉了（实测184ms的样本被剪成106ms）。改成只掐开头、保守阈值，
      并加上时长校验
修正：事件钩子从 kCGHIDEventTap（HID最底层）改到 kCGSessionEventTap（会话层）。
      前者一旦回调卡住会把整个系统的输入处理拖住，连鼠标焦点都会失灵
修正：事件钩子回调里的 assert! 改成提前返回 —— 在钩子里 panic 会 unwind
      穿过 CoreGraphics 的 C 栈，可能让钩子卡死
修改：不再在 app 变成活跃时自动弹设置窗口。它会反复抢焦点，打断用户正在
      使用的其它程序。设置窗口改由菜单栏图标或 QAZ123 打开
修改：LSUIElement 保持 true（纯菜单栏程序，不占 Dock），与原版行为一致
修正：启动后界面上什么都不出现，用户以为没打开。上一版改成纯菜单栏程序
      （不占 Dock）又把自动弹设置窗口一并去掉了，双击后唯一的反馈只剩菜单栏
      一个小图标。现在启动时会显示设置窗口，作为明确的反馈

0.5.0
增加“爆裂鼓手”音效
增加排除列表
设置界面改变
再次运行程序自动打开设置界面

0.4.2
修正系统睡眠恢复后失效问题

0.4.1
修正因在10.11下编译导致10.10中无法运行的问题

0.4.0
修正快速输入时声音不连贯问题;
检查更新显示更新内容
增加2款Cherry音效