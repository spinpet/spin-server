#!/usr/bin/env node

// S30 K线 WebSocket 持续监听脚本
// 只订阅 s30 间隔的 K线数据并持续接收

const { io } = require('socket.io-client');

// 配置
const SERVER_URL = 'http://localhost:5051';
const TEST_MINT = 'DTyWzeXUXXaYqJAbqP3J2wS4WJHrBz9NauNi63hBdjQP'; // 测试用的 mint 地址
const INTERVAL = 's30';

console.log('🚀 启动 S30 K线持续监听...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`🪙 代币地址: ${TEST_MINT}`);
console.log(`⏰ 监听间隔: ${INTERVAL}`);

// 创建 Socket.IO 客户端 - 连接到 /kline 命名空间
const socket = io(`${SERVER_URL}/kline`, {
    transports: ['websocket', 'polling'],
    timeout: 20000,
    reconnection: true,
    reconnectionAttempts: 10,
    reconnectionDelay: 2000,
});

// 连接事件监听
socket.on('connect', () => {
    console.log('✅ 连接成功');
    console.log(`🔌 Socket ID: ${socket.id}`);
    
    // 等待连接稳定后订阅
    setTimeout(() => {
        subscribeS30();
    }, 1000);
});

socket.on('disconnect', (reason) => {
    console.log(`❌ 连接断开: ${reason}`);
    console.log('🔄 等待重连...');
});

socket.on('connect_error', (error) => {
    console.log(`💥 连接错误: ${error.message}`);
});

// 接收服务器消息
socket.on('connection_success', (data) => {
    console.log('🎉 收到连接成功消息:', JSON.stringify(data, null, 2));
});

socket.on('subscription_confirmed', (data) => {
    console.log('✅ S30订阅确认:', JSON.stringify(data, null, 2));
});

socket.on('history_data', (data) => {
    if (data.interval === INTERVAL) {
        console.log(`📈 S30历史数据:`, {
            symbol: data.symbol,
            interval: data.interval,
            dataPoints: data.data.length,
            hasMore: data.has_more,
            totalCount: data.total_count
        });
        
        if (data.data.length > 0) {
            console.log('   最新K线:', data.data[0]);
        }
    }
});

socket.on('kline_data', (data) => {
    if (data.interval === INTERVAL) {
        const klineTime = new Date(data.data.time * 1000);
        console.log(`📊 S30实时K线更新:`, {
            symbol: data.symbol,
            time: klineTime.toISOString(),
            开盘价: data.data.open,
            最高价: data.data.high,
            最低价: data.data.low,
            收盘价: data.data.close,
            成交量: data.data.volume,
            更新类型: data.data.update_type,
            更新次数: data.data.update_count,
            接收时间: new Date(data.timestamp).toISOString()
        });
    }
});

socket.on('error', (error) => {
    console.log('❌ 错误消息:', JSON.stringify(error, null, 2));
});

// 订阅 S30 数据
function subscribeS30() {
    console.log('\n📊 订阅 S30 K线数据...');
    socket.emit('subscribe', {
        symbol: TEST_MINT,
        interval: INTERVAL,
        subscription_id: `s30_monitor_${Date.now()}`
    });
    
    // 可选：获取一些历史数据作为参考
    setTimeout(() => {
        console.log('📈 获取最近10条S30历史数据...');
        socket.emit('history', {
            symbol: TEST_MINT,
            interval: INTERVAL,
            limit: 10
        });
    }, 2000);
}

// 错误处理
process.on('unhandledRejection', (reason, promise) => {
    console.log('Unhandled Rejection at:', promise, 'reason:', reason);
});

process.on('uncaughtException', (error) => {
    console.log('Uncaught Exception:', error);
    process.exit(1);
});

// 优雅退出
process.on('SIGINT', () => {
    console.log('\n👋 收到退出信号，正在断开连接...');
    socket.disconnect();
    process.exit(0);
});

console.log('\n📋 功能说明:');
console.log('  - 连接到 WebSocket 服务器');
console.log('  - 订阅指定代币的 S30 K线数据');
console.log('  - 持续接收并显示实时更新');
console.log('  - 按 Ctrl+C 退出监听\n');