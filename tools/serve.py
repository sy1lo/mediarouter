"""dist 的静态服务，供 Edge 无头验证使用。

为什么不用 `python -m http.server`：

  Windows 上 `mimetypes.guess_type('.svg')` 会走注册表，可能给出 **`image/svg`**
  —— 这不是标准 MIME（标准是 `image/svg+xml`）。Chromium 对 `<img>` 的 MIME
  是严格校验的，遇到 `image/svg` 直接判定为不可解码，触发 error 事件。

  实测后果非常隐蔽：图片全部走"加载失败 → 退回缩略图"的降级分支，
  于是**看起来页面有图**，但每张图的宽高比都是缩略图的，
  平铺布局的验证会得到一个假的"高度变形"结论。踩过一次，这里显式钉住 MIME。

用法：
  python tools/serve.py [端口]        # 默认 8790，服务 dist/
"""

import http.server
import mimetypes
import os
import socketserver
import sys

# 必须在建 server 之前注册，覆盖注册表里那个错的映射
mimetypes.add_type("image/svg+xml", ".svg")
mimetypes.add_type("image/webp", ".webp")
mimetypes.add_type("image/avif", ".avif")

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8790
DIST = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "dist")
os.chdir(DIST)


class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):  # 别把访问日志刷到终端
        pass


socketserver.TCPServer.allow_reuse_address = True
with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
    print(f"serving {os.path.abspath(DIST)} on http://127.0.0.1:{PORT}")
    httpd.serve_forever()
