import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

// 后端本地地址（helm-server dev 默认 8080；若被占用可临时切走）
const BACKEND = 'http://127.0.0.1:8080'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    // 0.0.0.0：局域网设备可访问（Vite 默认仅 loopback）
    host: '0.0.0.0',
    proxy: {
      // HTTP API（含 /healthz 之外的 /api/v1/*）
      '/api': {
        target: BACKEND,
        changeOrigin: true,
        // WebSocket 端点（/api/v1/*/stream、terminal）——同路径升级，ws: true 一并转发
        ws: true,
      },
      '/healthz': {
        target: BACKEND,
        changeOrigin: true,
      },
      // MCP 连接测试（不能用 /mcp：那会劫持同名 SPA 前端路由的页面导航 → 405）
      '/mcp-test': {
        target: BACKEND,
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/mcp-test/, '/mcp'),
      },
    },
  },
})
