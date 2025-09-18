#!/bin/bash

# K线 WebSocket 服务测试启动脚本

echo "🚀 启动 K线 WebSocket 服务测试..."

# 检查依赖
echo "📦 检查依赖..."
if ! command -v node &> /dev/null; then
    echo "❌ Node.js 未安装，请先安装 Node.js"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo 未安装，请先安装 Rust"
    exit 1
fi

# 安装 socket.io-client（如果未安装）
if ! npm list socket.io-client &> /dev/null; then
    echo "📥 安装 socket.io-client..."
    npm install socket.io-client
fi

# 构建项目
echo "🔨 构建 Rust 项目..."
cargo build --release

if [ $? -ne 0 ]; then
    echo "❌ 构建失败，请检查编译错误"
    exit 1
fi

echo "✅ 构建成功"

# 启动服务器（后台运行）
echo "🚀 启动 K线 WebSocket 服务器..."
cargo run --release &
SERVER_PID=$!

# 等待服务器启动
echo "⏳ 等待服务器启动..."
sleep 5

# 检查服务器是否启动成功
if ! curl -f http://localhost:5051/api/time > /dev/null 2>&1; then
    echo "❌ 服务器启动失败，请检查日志"
    kill $SERVER_PID 2>/dev/null
    exit 1
fi

echo "✅ 服务器启动成功"

# 等待用户准备
echo ""
echo "🎯 服务器已启动，现在可以运行测试："
echo "   WebSocket 端点: ws://localhost:5051/kline"
echo "   API 状态: http://localhost:5051/api/kline/status"
echo "   API 文档: http://localhost:5051/swagger-ui"
echo ""
echo "按任意键开始 WebSocket 测试，或 Ctrl+C 退出..."
read -n 1 -s

# 运行 WebSocket 测试
echo ""
echo "🧪 运行 WebSocket 测试..."
node test_kline_websocket.js

# 清理
echo ""
echo "🧹 清理进程..."
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo "✅ 测试完成"