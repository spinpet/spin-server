#!/bin/bash

# 测试脚本 - 验证 Spin Server 的功能

echo "🧪 开始测试 Spin Server..."

# 启动服务器
echo "🚀 启动服务器..."
cargo run &
SERVER_PID=$!

# 等待服务器启动
sleep 3

# 测试基本时间接口
echo "⏰ 测试基本时间接口..."
response=$(curl -s http://127.0.0.1:8080/api/time)
if [[ $response == *"timestamp"* ]]; then
    echo "✅ 基本时间接口测试通过"
else
    echo "❌ 基本时间接口测试失败"
fi

# 测试带格式的时间接口
echo "🕐 测试带格式的时间接口..."
response=$(curl -s "http://127.0.0.1:8080/api/time?format=%H:%M:%S")
if [[ $response == *"timestamp"* ]]; then
    echo "✅ 带格式时间接口测试通过"
else
    echo "❌ 带格式时间接口测试失败"
fi

# 测试OpenAPI文档
echo "📚 测试OpenAPI文档..."
response=$(curl -s http://127.0.0.1:8080/api-docs/openapi.json)
if [[ $response == *"/api/time"* ]]; then
    echo "✅ OpenAPI文档测试通过"
else
    echo "❌ OpenAPI文档测试失败"
fi

# 测试Swagger UI
echo "📖 测试Swagger UI..."
response=$(curl -s http://127.0.0.1:8080/swagger-ui)
if [[ $response == *"Spin API Documentation"* ]]; then
    echo "✅ Swagger UI测试通过"
else
    echo "❌ Swagger UI测试失败"
fi

# 确认不需要的接口已被删除
echo "🔍 测试不需要的接口是否已删除..."
health_response=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8080/api/health)
spin_response=$(curl -s -o /dev/null -w "%{http_code}" http://127.0.0.1:8080/api/spin/info)

if [[ $health_response == "404" ]] && [[ $spin_response == "404" ]]; then
    echo "✅ 不需要的接口已成功删除"
else
    echo "❌ 不需要的接口删除失败"
fi

# 停止服务器
echo "🛑 停止服务器..."
kill $SERVER_PID

echo "🎉 测试完成！" 