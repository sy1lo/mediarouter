# 工程笔记 — Tauri v2 重写

记的是实现过程中的设计取舍、踩到的上游问题，以及那些「看着像 bug 但其实是刻意的」细节。
想改这个项目之前值得先读一遍。

前身是一版基于 Electron 的实现，本目录是换栈重写后的版本。

---

## 1. 结论先说

**能编译、能链接、能打包、类型检查全绿。运行时行为未经真机验证**（原因见 §6）。

| 验证项 | 状态 | 证据 |
|---|---|---|
| 前端类型检查 | ✅ 通过 | `vue-tsc --noEmit` exit=0 |
| 前端构建 | ✅ 通过 | `vite build` 2715 modules，5.07s |
| Rust 编译检查 | ✅ 通过 | `cargo check` exit=0，**0 错 0 警** |
| Rust 链接（MSVC） | ✅ 通过 | `cargo build` exit=0 → `target/debug/mediarouter.exe` 21.6 MB |
| 配置/权限/图标校验 | ✅ 通过 | `tauri-build` 在编译期校验 `tauri.conf.json` 与 capabilities |
| **运行时行为** | ❌ **未验证** | 本机环境会回收我启动的 GUI 进程（§6） |
| **纯 Rust COM 系统缩略图** | ❌ **未验证** | 只验证了能编译；能否真取出缩略图必须真机跑 |

---

## 2. 移植地图

| Electron | Tauri | 说明 |
|---|---|---|
| `main.js` | `lib.rs` + `paths.rs` + `state.rs` | 生命周期、数据目录、窗口显示、`thumb` 协议 |
| `ipc.js` + `preload.js` | `commands.rs` | 22 个 `#[tauri::command]`，去掉 preload 中间层 |
| `store.js` | `store.rs` | JSON 原子写入；状态收进 `AppState` |
| `scanner.js` | `scanner.rs` | 扩展名白名单、递归深度 6、作品集聚合 |
| `metadata.js` | `metadata.rs` | ffmpeg 探测顺序、时长查询与缓存 |
| `router.js` | `launcher.rs` | 规则匹配、参数模板、分离进程启动 |
| `thumbs.js` | `thumbs/{mod,epub,shell,mime}.rs` | 哈希命名、并发闸门、epub 六级级联 |
| `shell-thumb.ps1` | `thumbs/shell.rs` | **整条替换**：PowerShell + GDI+ 反射编译 → 纯 Rust COM |
| 渲染层 pdf.js | 不变 | WebView2 里有 canvas 与 worker，**Rust 侧完全不碰 PDF** |
| 前端 16 个 `.vue` | 不变 | 只改 import 与调用点 |

---

## 3. 关键设计决定

### 3.1 PDF 缩略图留在渲染层 —— 本次最大的风险规避

Electron 版本来就是渲染层的 pdf.js 生成 PDF 缩略图（`FileThumb.vue`），
回传后端落盘。WebView2 里 canvas 与 worker 都在，所以这段**一行都不用改**，
Rust 侧因此**不需要引入任何 PDF 渲染器**。

后端对 `.pdf` 直接返回「生成不了」，前端渲染完回调 `file_save_thumb` ——
与旧版契约一致。

### 3.2 系统缩略图改纯 Rust COM

旧方案：起 `powershell.exe`（冷启动 300–800 ms）→ `Add-Type` 把一段 C# 源码
**反射编译成 DLL**（首次落盘缓存）→ P/Invoke 调同一个 Shell 接口 → GDI+ 存 PNG。

新方案：直接调 COM，无子进程、无反射编译、无 DLL 落盘。

```
SHCreateItemFromParsingName::<_, Option<&IBindCtx>, IShellItemImageFactory>(path, None)
  → factory.GetImage(SIZE{cx,cy}, flags)       // 四档降级
  → GetObjectW + GetDIBits（取 32bpp 自顶向下 BGRA）
  → BGRA→RGBA 后 PngEncoder 编码落盘
```

四档 flags 与旧脚本一致：`BIGGERSIZEOK|THUMBNAILONLY` → `THUMBNAILONLY`
→ `RESIZETOFIT` → `ICONONLY`。

**一个实测到的坑**：Shell 返回的 HBITMAP 经常整幅 `alpha=0`（明明是不透明图）。
代码里的处理是「只有在**一个非零 alpha 都没有**时才强制不透明」，
这样既不误伤真有透明通道的图标，又能修掉常见的全零情况。

### 3.3 epub 封面级联有个「看着像 bug 但不能修」的细节

旧版候选封面带了 `score` 字段（封面属性 100 / meta 引用 95 / id=cover 90），
**但这个 score 从未参与排序** —— 旧版是按「推入顺序」依次尝试的。
所以真实优先级是：

```
properties="…cover-image…"  >  id="cover"  >  meta name="cover"
```

移植时**必须复刻同样的顺序**，不能顺手「修成按分数排序」，否则行为会变。
代码里对此有显式注释。

### 3.4 排序职责搬到前端

旧版在主进程用 Node 的 ICU 做 `localeCompare('zh-CN', {numeric: true})`。
Rust 侧要拿到等价的拼音排序就得拖进 ICU 数据。改由前端 `Intl.Collator` 完成：
WebView2 自带完整 ICU，结果与旧版一致。

**顺带修掉一个旧版的不一致**：`scanner.js` 里只对 `loose`（根目录散件）调了
`.sort()`，**作品集内的文件从来没排序过**（按 `readdir` 顺序展示）。
现在两者都排。

### 3.5 缩略图缓存的哈希与版本号一字未改

`md5(绝对路径 + "\0" + "v2")`，`THUMB_VERSION = "v2"`。
旧版的 `thumbs/` 目录可以直接搬过来继承（已核对旧数据的 `.version` 内容就是 `v2`）。

### 3.6 缩略图并发的两处改动

- **并发上限 2 → 4**：旧版压到 2 是为了控制 Node 侧的内存峰值；
  Rust 每张图的驻留内存小一个量级，没必要继续付这个代价。
- **去重方式**：旧的「队列 + 单例 promise」换成「拿到信号量后复查磁盘」——
  并发请求同一文件时，后到的那个复查发现文件已存在，直接返回。无锁、无状态。

### 3.7 数据目录

沿用绿色语义，但判定依据从环境变量换成 exe 自身位置：

1. `<exe 所在目录>/MediaRouterData`（可写则用；判据是实际写一个探针文件，
   不是只判存在性——否则会踩到 Program Files 的权限坑）
2. 回退 `%APPDATA%\com.medialib.launcher`

旧版依赖 `PORTABLE_EXECUTABLE_DIR`，那个变量由 NSIS 便携壳注入，
换成 zip / 安装包分发后**永远为空**，所以必须换。

---

## 4. 环境搭建（可复用）

### 4.1 Rust 工具链

```bash
# 1) rustup（装到用户目录，无需管理员）
curl -L -o rustup-init.exe https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe
./rustup-init.exe -y --default-toolchain stable --profile default --default-host x86_64-pc-windows-msvc
```

> ⚠️ 本次安装后 `~/.cargo/bin/*.exe` **全是 0 字节**（shim 创建失败）。
> 症状很隐蔽：`rustc --version` **静默退出、零输出、exit 0**。
> 修法：shim 本质就是 `rustup.exe` 的副本（靠 argv\[0\] 分派），
> `cp -f rustup.exe cargo.exe && cp -f rustup.exe rustc.exe && …` 逐个覆盖。

### 4.2 MSVC + Windows SDK

```bash
./vs_BuildTools.exe --quiet --wait --norestart \
  --installPath "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools" \
  --add Microsoft.VisualStudio.Workload.VCTools \
  --add Microsoft.VisualStudio.Component.VC.Tools.x86.x64 --includeRecommended
```

用 **Bash 直接跑**（`Start-Process -Verb RunAs` 会被沙箱层内部错误拦下）。
装完得到 `VC/Tools/MSVC/14.44.35207` + `Windows Kits/10/Include/10.0.26100.0`。

> 别被 Git Bash 的 `/usr/bin/link.exe` 骗了 —— 那是 coreutils 的硬链接工具，
> **不是** MSVC 链接器。要查得
> `find "<BuildTools>/VC/Tools/MSVC" -name link.exe -path "*x64*"`。

### 4.3 前端依赖

```bash
npm install --ignore-scripts
```

> ⚠️ **必须加 `--ignore-scripts`。** 本机 `esbuild` 的 postinstall 会因
> `spawnSync(<托管node.exe>) EBUSY` 失败，导致整个 install 中断并留下
> **残缺的 `node_modules` + `package-lock.json`**；之后再跑 `npm install`
> 会按 lock 认为已完整、直接跳过缺失的可选依赖（`@esbuild/win32-x64`、
> `@rollup/rollup-win32-x64-msvc`），现象是 vite 构建失败而 npm 只说
> "added 2 packages"。
>
> `env -u NODE_OPTIONS` 与关闭沙箱**都不能解决**；跳过 postinstall 才行
> （esbuild 的真实二进制、rollup 的原生模块、tauri-cli 的平台二进制
> 都来自可选依赖包，不依赖 postinstall）。
>
> 若已踩坑：先把 `node_modules` 和 `package-lock.json` **移走**再重装。

---

## 5. 构建期踩到的两个上游问题

### 5.1 `schemars 0.8` × `indexmap 1.9.3` 编译失败（已在 Cargo.toml 修补）

```
schemars-0.8.22/src/lib.rs:12:32: error[E0107]:
struct takes 3 generic arguments but 2 generic arguments were supplied
```

链路：`tauri-build` → `tauri-utils` → `schemars 0.8`（生成配置 schema）。

`schemars 0.8` 在 `preserve_order` 特性下写的是
`type Map<K,V> = indexmap::IndexMap<K,V>`（两个泛型参数），而 indexmap 只在
启用 `std` 时第三个参数（hasher）才有默认值：

```rust
#[cfg(feature = "std")]      pub struct IndexMap<K, V, S = RandomState>
#[cfg(not(feature = "std"))] pub struct IndexMap<K, V, S>
```

**致命点是 `indexmap 1.9.3` 的 `[features]` 里没有 `default = ["std"]`**
（只有 `std = []`），所以即便别处以默认特性引入它，`std` 也不会被打开。

修法：在 `Cargo.toml` 里加一条直接依赖把 `std` 强制打开。**关键是必须放两处**：

```toml
[build-dependencies]
indexmap = { version = "1", default-features = false, features = ["std"] }
[dependencies]
indexmap = { version = "1", default-features = false, features = ["std"] }
```

因为 **resolver v2 不在「构建依赖」与「普通依赖」之间合并特性**，
而 schemars 是从构建依赖链引入的。只放 `[dependencies]` 无效
（现象：改动看似生效、错误一模一样）。

等上游修复后可删。

### 5.2 windows-rs 的两个 API 与直觉不符

```rust
// ① 是 3 个泛型参数，且第二个要显式写成 Option<&IBindCtx>
SHCreateItemFromParsingName::<_, Option<&IBindCtx>, IShellItemImageFactory>(path, None)

// ② COM 的 [out] 参数被改写成了返回值，不要自己传 &mut HBITMAP
factory.GetImage(SIZE{cx,cy}, flags) -> Result<HBITMAP>
```

### 5.3 regex 的 `Captures` 索引是 `str` 而不是 `&str`

```rust
// impl<'h> Index<usize> for Captures<'h> { type Output = str; }   ← 注意是 str
by_id.get(&c[1])       // ✅ 必须取引用
by_id.get(c[1].as_str())  // ❌ 解析到未稳定的 str::as_str（E0658）
by_id.get(c[1].as_ref())  // ❌ str 有多个 AsRef 实现，类型歧义（E0283）
```

> 通用经验：**核实第三方 crate 的 API 不要靠猜**。`cargo fetch` 之后源码就在
> `~/.cargo/registry/src/index.crates.io-*/<crate>-<版本>/`，Grep 读真迹
> 比记忆可靠一个量级。本次靠它确认了 `UriSchemeContext::app_handle()`、
> `ipc::Response::new` 的 `From<Vec<u8>>`、dialog 的 `blocking_pick_file`、
> `FilePath::into_path`、以及上面这三处。

---

## 6. 为什么运行时行为没验证

**本机环境会回收我启动的 GUI 进程。** Bash / PowerShell 启动的 exe 会
**秒退、零 stderr、Windows 事件日志无任何崩溃记录**。

判定方法是拿一个**已知可用**的旧构建做对照 —— 旧构建在同一环境下同样秒退，
所以不是代码问题。试过沙箱内 / 绕沙箱 / `nohup &` / `Start-Process` 四种方式，
全部失败。

**因此：冷启动耗时、界面表现、缩略图能否生成，必须由用户双击实测。**

---

## 7. 用户需要做的验证

```bash
# 1. 把旧数据搬到新 exe 旁边（继承 371 张缩略图与规则）
cp -r ../main/release/MediaRouterData src-tauri/target/debug/MediaRouterData

# 2. 双击
src-tauri/target/debug/mediarouter.exe
```

要看的四件事：

1. **窗口几秒出现**（应该 < 2s）
2. **漫画库 15 个作品集照常显示**（验证数据继承）
3. **缩略图能否出来**（重点看非 zip/epub 的类型 —— 那些走纯 Rust COM）
4. **双击文件能否按规则拉起外部程序**

若窗口是空白的，**第一个要怀疑的是 CSP**（`tauri.conf.json` 的 `app.security.csp`）；
临时把它设为 `null` 可以快速定位。

---

## 8. 第二轮修复：动漫板块每次重启都重新检测时长与封面

用户反馈：**动漫分类每次重启应用都会重新检测加载时长和封面。**

### 诊断过程（先取证，不猜）

三个假设被逐一验证/推翻，值得记录下来，因为它们说明了「怎么查」比「查到什么」更重要：

| 假设 | 验证方式 | 结论 |
|---|---|---|
| 缩略图哈希算法与旧数据不一致，导致缓存全部失效 | 用 Node 复现 `md5(path + "\0" + "v2")`，逐个核对 84 个动漫文件 | ❌ **推翻**：84/84 全部命中磁盘缓存 |
| 用户配置的 ffmpeg 路径不对（发行包结构是 `bin/ffmpeg.exe`） | 实际 `ls` 该目录 | ❌ **推翻**：用户那份是扁平结构，`ffmpeg.exe` 就在根下 |
| 缩略图其实走的不是 ffmpeg 抽帧 | **看缩略图的魔数** | ✅ **命中**：84 个全是 PNG，ffmpeg 一次都没生效 |

真正的突破口是「看文件魔数」——ffmpeg 抽帧输出 JPEG，而 `shell.rs` 输出 PNG。
一行判据就把问题锁死了。

### 三个真实缺陷

**(a) `run_with_timeout` 在 stdout 为 null 时直接短路** ← 主因

```rust
pub fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<String> {
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;   // ← stdout 被设成 null() 时这里是 None
```

而 `ffmpeg_thumb` 不需要读 ffmpeg 的输出，设的是 `.stdout(Stdio::null())`
→ 这个 `?` 立刻返回 `None` → `ffmpeg_thumb` 恒返回 false
→ **所有视频缩略图都掉进了系统缩略图回退**。

顺带还有两个副作用：被 spawn 出来的 ffmpeg 进程无人 `wait`，
可能在新图写完后才落盘，与 `shell::system_thumbnail` 写同一个文件产生竞态。

**实测差距：**

| | 体积 | 魔数 |
|---|---|---|
| ffmpeg 抽帧（应该走的路） | **16,881 字节** | `ff d8 ff` (JPEG) |
| 实际缓存的系统缩略图 | **874,803 字节** | `89 50 4e` (PNG) |

**相差 51.8 倍。** 而且系统缩略图没有被压到请求的尺寸——
因为级联第一档带了 `SIIGBF_BIGGERSIZEOK`，那是**主动允许** Shell 返回更大的图。

**(b) 时长缓存只在内存里**

`MetaCache.duration` 与前端 `metaCache.duration` 都是会话级，
每次重启都要对每个视频重跑 ffprobe。更糟的是 `file_meta` 当时是
**在 async 命令里同步跑 ffprobe**——一个阻塞调用（最长 12s）占着
异步工作线程，几十个视频同时请求会把 `file_thumb` 等 IPC 一起拖住。

**(c) 系统缩略图输出无上限**

即便回退到 Shell，也应该把结果裁剪到目标尺寸、并在不透明时用 JPEG——
875KB 的无损 PNG 对「一屏几十张图」来说是灾难。

### 对应的四处修改

1. **`metadata.rs`**：拆成两个函数
   - `run_to_completion(cmd, timeout)` —— 轮询 `try_wait`，对输出配置无要求
   - `run_capturing_stdout(cmd, timeout)` —— 捕获输出，**要求**调用方设 `piped()`
   `ffmpeg_thumb` 改用前者，并检查 `status.success()` 与文件非空。

2. **`metadata.rs` + `state.rs`**：时长缓存落盘到 `<数据目录>/meta.json`
   - 条目 = `{ mtime, size, duration }`，用 **mtime+size** 而非只靠路径做失效判定
     （视频被重新压制后时长会变）
   - 写入带 2 秒节流，避免每个文件一次磁盘写；**退出时（`RunEvent::Exit`）强制落盘**，
     否则「探测完就关窗」的那批会丢
   - `state.meta` 改为 `Arc<Mutex<MetaCache>>`，`file_meta` 用 `spawn_blocking` 跑 ffprobe

3. **`thumbs/mod.rs`**：`THUMB_VERSION` 由 `"v2"` 提到 `"v3"`。
   **必需**——旧缓存里躺着 84 张错误的 PNG，不升版本它们会一直命中，
   修复永远不生效。哈希包含版本号，所以升版也顺带让 URL 变化，
   不会命中 WebView2 里旧的 `immutable` 响应缓存。

4. **`thumbs/shell.rs`**：
   - 级联第一档去掉 `SIIGBF_BIGGERSIZEOK`（不再主动索要超大图）
   - 新增尺寸封顶：超过目标尺寸就等比缩回
   - 不透明时落 **JPEG(q82)**，只有真带透明通道才落 PNG

### 修复后的预期

- 动漫封面由 875KB 无损 PNG → 约 17KB JPEG（**~50 倍**）
- 时长**首次运行后即持久化**，之后重启不再起任何 ffprobe
- 首次启动会因 `THUMB_VERSION` 升版**重建全部缩略图**——
  但缩略图是跟着 `IntersectionObserver` 懒加载生成的，
  所以代价分散在你浏览的过程中，不是一次性等几十秒

### 仍未做（有意为之）

- **替换文件内容不会刷新其缩略图**：哈希只含路径 + 版本号，不含 mtime。
  与旧版行为一致，改动它会破坏已有缓存的继承性。
- `meta.json` 不做清理：删除的文件会留下孤儿条目。
  它们无害（路径复用且 mtime/size 不同就会重新探测），只是文件会缓慢变大。
- 缩略图生成时定位抽帧点的 ffprobe 不走缓存（每个新文件多一次探测）。
  一次性代价，可忽略。

---

## 9. 第三轮优化：大量大体积 PDF 导致卡死

用户反馈：**打开含 70 个 PDF（最大单个 192 MiB）的作品集时卡得几乎不能动。**

### 诊断：三条根因

**(a) PDF 渲染队列是"假的"（最致命）**

原实现（`FileThumb.vue` 旧版）把 `pdfQueue` 写在 `<script setup>` 里，
Vue 会把它编进 `setup()`，于是**每个组件实例各有一份** `running / MAX = 1`。
而每个实例一辈子只 `enqueue` 一次（`load()` 由 `loaded` 守卫只跑一次），

> **⇒ `MAX = 1` 从未真正限流过。有效并发度 = 视口内触发的卡片数，可逼近 70。**

**(b) 每个 PDF 都要整份读进内存**

`readFileData` → `commands.rs` 的 `read_file_data` 是 `std::fs::read` 全量读，
峰值约 **270MB/文件**（Rust `Vec` + wry `SHCreateMemStream` 复制 + JS `ArrayBuffer`）。
而且它当时是 `pub async fn` 里直接跑阻塞 IO，**占住 tokio 工作线程**（worker 数 = CPU 核数），
连累所有其它 IPC。同项目里 `file_meta` 早就用 `spawn_blocking` 处理过同类问题，这里漏了。

**(c) 卸载不取消**：`onBeforeUnmount` 只断开了 IntersectionObserver，
在途的读取 / 解析 / 渲染 / 落盘全部照跑完，快速滚动时任务持续堆积。

**直接诱因**：上一轮把 `THUMB_VERSION` 从 v2 升到 v3，把**包括 PDF 在内的全部缩略图**作废重建了。

### 一个被证伪的方案（记下来避免重走）

最初的设想是「注册一个支持 Range 的自定义 `media` 协议 + `getDocument({ url })`」。
源码核实后**证伪**：pdf.js 的首次请求**不带 `Range` 头**，并且把响应的 `Content-Length`
当作整个文档长度 —— 所以第一次仍会整包拉满，而 wry 必须把响应体整体物化（无真正流式）。白搭。

只有 `getDocument({ range })` 这条路才**真的不发全量 GET**
（`PDFDataTransportStream.getFullReader()` 只消费我们给的 `initialData`，`pdf.mjs:9676-9681`）。

### pdf.js 契约（违反任意一条都会挂死或抛异常）

| # | 约束 | 证据 |
|---|---|---|
| 1 | `progressiveDone` **必须传 `true`** —— 传 `false` 时全量 reader 不会结束，会一直等一个我们不会提供的「渐进流」 | `pdf.mjs:11572`、`:9702` |
| 2 | 必须用 pdf.js 传进来的 **`begin` 原样回数据** | `pdf.mjs:9640-9649` 的 `assert` |
| 3 | **绝不能在 `requestDataRange` 里同步回调 `onDataRange`** —— `getRangeReader` 是先调 `requestDataRange`、再压读器 | `pdf.mjs:9686-9688` |
| 4 | `end` 是**排他**上界 | `pdf.worker.mjs:1938-1939` |
| 5 | **短包是致命的** —— worker 按 `begin + byteLength` 记账，缺失 chunk 永不重发，加载永久挂起 | `pdf.worker.mjs:2027-2058` |
| 6 | **没有 error 通道** —— 失败必须靠 `fatal` + `Promise.race` + `destroy()` 兜 | `pdf.mjs:11595-11627` |
| 7 | `doc.destroy()` 会走到我们的 `abort()`（基类是空实现） | `pdf.mjs:9691-9697`、`:11627` |

> 约束 2 与 3 都用**反例跑通了**：harness 里故意用 `begin + 1` 回数据，
> 确实观察到 pdf.js 抛 assert。这说明它不是"理论上应该"，而是真实存在的。

### 实测数据（这是本轮最有价值的部分）

用 `scripts/verify-range-transport.mjs` 在**用户的真实 PDF** 上测的。

**第一件事：`rangeChunkSize` 的直觉是反的。**
最初的实现选了 256 KiB，理由是"减少 IPC 轮次"。实测证明这是**纯粹的反向优化** ——
解析器发起的是**零散的对象请求**，每笔都按 chunk 向上取整，所以
`读取量 ≈ 请求数 × chunkSize`，chunk 越大，为一个小对象读进的废数据越多。

192 MiB 图片型 PDF（预置头部 64 KiB）：

| 粒度 | 请求 | 读取 | 占比 |
|---|---|---|---|
| 256 KiB | 161 | 40.10 MiB | 20.87% |
| 64 KiB | 161 | 10.04 MiB | 5.22% |
| **16 KiB（采用）** | 163 | **2.74 MiB** | **1.43%** |
| 8 KiB | 164 | 1.37 MiB | 0.71% |
| 4 KiB | 165 | 0.74 MiB | 0.39% |

**请求数几乎不变（161→165），而读取量降 54 倍。**
不再往 4 KiB 取的原因：另一本**纯文字型** PDF 上，4 KiB 会把请求数从 16 KiB 的
16 次推到 54 次（对象更零散），而收益仅 0.04 MiB。

**第二件事：`initialData` 不能贪多。**

| 预置头部 | 大漫画真实总量 | 文字型真实总量 |
|---|---|---|
| 2048 KiB | 2.63 MiB | 2.21 MiB |
| 512 KiB | 1.13 MiB | 0.71 MiB |
| 192 KiB | 0.93 MiB | 0.40 MiB |
| **64 KiB（采用）** | **0.93 MiB** | **0.27 MiB** |
| 16 KiB | 0.93 MiB | 0.25 MiB |

超过 ~192 KiB 就是纯过度预读（**2 MiB 的预读量已经超过整页所需**）。
16–192 KiB 区间内大文件总量几乎相同（多预读就少发请求，互相抵消）。

> ⚠️ 这个表第一版是错的 —— 当时的统计**没把预置头部计入总量**，
> 于是得出"头部越大越好"的反向结论。**统计口径本身就是 bug 来源，值得专门核一遍。**

**定稿参数下的三本实测（全 PASS）：**

| 文件 | 大小 | 实际读取 | 占比 | 请求 | 耗时 |
|---|---|---|---|---|---|
| 咒术回战 28（图片型） | 192.10 MiB | **2.74 MiB** | **1.43%** | 163 | 121 ms |
| 一拳超人 Vol_01（图片型） | 67.95 MiB | 0.25 MiB | 0.37% | 8 | 63 ms |
| 毛泽东选集（**纯文字型**） | 5.22 MiB | 0.25 MiB | 4.79% | 16 | 23 ms |

**192 MiB → 2.74 MiB，约 70 倍。**

### 怎么观察到这次改进

**注意：已有的 PDF 缩略图已经缓存在磁盘上，`file_thumb` 会直接命中，
前端根本不会走到新的 range 代码 —— 所以改进默认不会自动显现。**

要观察，需要在设置页点**「清空缩略图」**（它会同时清掉后端磁盘缓存与前端 `metaCache`），
然后重新打开那个作品集。预期：

- 原来：整屏几十张卡片同时各自拉 90MB，界面假死
- 现在：全局并发 2，每张只读约 2.7 MiB，首屏在 1–2 秒内出图

按实测的单文件 ~120ms 估算，70 个 PDF 的总工作量约 4–9 秒，且是**跟着滚动懒加载**的，
不是一次性等待。

日常使用中，这个改进会在**新增 PDF、或将来再升缓存版本时**自动生效。

### 改动清单

| 文件 | 改动 |
|---|---|
| `src-tauri/src/commands.rs` | 新增 `allowed_roots` / `resolve_allowed_file` / `read_range_bytes` / `read_file_range`；`read_file_data` 补路径白名单 + `spawn_blocking`；新增 `#[cfg(test)]` 4 个用例 |
| `src-tauri/src/lib.rs` | `generate_handler!` 注册 `read_file_range` |
| `src/api/index.ts` | 新增 `readFileRange`，并校验返回长度必须等于 `end - begin` |
| `src/utils/pdfTaskQueue.ts` | **新建**：模块级单例队列（并发 2、排队期取消不占名额、不饥饿） |
| `src/utils/pdfRange.ts` | **新建**：`RangeTransport` + `renderPdfThumb`（小文件整份读 / 大文件 range） |
| `src/stores/cache.ts` | 新增会话级 `pdfFailed`，防止病态 PDF 反复重试 |
| `src/components/FileThumb.vue` | 走 `runPdfTask`、加 loading 指示、固定卸载清理顺序、**即使卸载仍落盘缩略图** |
| `scripts/verify-pdf-queue.ts` | **新建**：队列语义验证（14 项） |
| `scripts/verify-range-transport.mjs` | **新建**：pdf.js 契约 + 读取量实测 |

### 两个刻意的设计决定

**① 路径校验不做 `path.contains("..")` 子串检查。**
媒体库里 `第01话..修正版.mp4` 这类文件名很常见，子串检查会误杀合法文件。
`canonicalize` 已经把 `..` 完全解开，"解开的绝对路径是否落在某个根目录内"才是真正严密的防线。
（`#[cfg(test)]` 里专门有一个用例守着这条。）

**② 取消后仍落盘缩略图。**
昂贵的是 `getPage + render + toDataURL`，一旦走到落盘说明钱已经付过。
`saveThumb` 成功后会 `thumbs.unfail`，下次滚到该文件时 `file_thumb` 直接命中磁盘缓存、
零 PDF 工作 —— 等于把「快速滚动造成的瞬时工作」沉淀成「永久命中」。

### 验证方式（本机无法启动 GUI）

- `scripts/verify-range-transport.mjs` —— 用 `pdfjs-dist/legacy/build/pdf.mjs` 在**纯 Node** 里
  跑真实 PDF，用文件读取型 transport 把每条契约变成可证伪的断言。
  用 `getOperatorList()` 代替 `page.render()`（渲染需要 canvas）。
- `scripts/verify-pdf-queue.ts` —— `node --experimental-strip-types` 直跑（队列模块不依赖 Vue/DOM）。
- `cargo test` —— 区间字节语义 + 路径校验。
- `vue-tsc --noEmit` / `vite build` / `cargo check`。

### 仍未验证

- **WebView2 里的真实表现**（滚动 70 卡片的 CPU/内存）—— 必须用户双击实测。
- `read_file_range` 经由 Tauri `ipc::Response` → 前端 `ArrayBuffer` 的真实链路：
  与既有 `read_file_data` 走同一机制，属低风险未测项。
- **内存峰值只降到 ~180MB**（并发 2 × 90MB）：pdf.js 的 worker 仍按**文件全长**
  预分配 `ChunkedStream` 缓冲（`pdf.worker.mjs:1664-1666`）。range 省掉的是
  **磁盘 I/O 与 IPC 传输**，不是 worker 堆占用。这一点无法靠本方案绕过。

---

## 10. 遗留项与下一步

- [ ] **真机验证**（§7）—— 唯一挡在路上的事
- [ ] 若系统缩略图（`thumbs/shell.rs`）出不来图，回退方案是起旧版的
      `shell-thumb.ps1`（约 20 行，`Command::new("powershell.exe").args([...])`）
- [ ] `.cbr`/`.cbt`（RAR）无内置封面提取，仍靠系统缩略图 —— 与旧版一致
- [ ] 扫描结果仍未落盘（每次启动全量重建）—— 旧版就是这样，本地库规模下 < 1s
- [ ] 大文件（PDF）走 `ipc::Response` 原始字节，比旧版少两次拷贝，
      但仍是「整文件进内存」。彻底解法是注册一个支持 Range 的 `media` 协议，
      让 pdf.js 直接按 URL 流式拉取
- [ ] NSIS 安装包未签名，首次运行可能被 SmartScreen 拦
- [ ] `ffmpeg` 未内置，需用户自备
- [ ] 顶层 `../README.md`、`../HANDOVER.md`、`../ARCHITECTURE.md` 描述的仍是
      Electron 版；**两套实现并存时这三份文档的定位需要用户决定**
