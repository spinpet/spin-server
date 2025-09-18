#!/usr/bin/env node

// K线 WebSocket 测试脚本
// 测试实时 K线数据订阅功能

const { io } = require('socket.io-client');

// 配置
const SERVER_URL = 'http://localhost:5051';
const TEST_MINT = 'DTyWzeXUXXaYqJAbqP3J2wS4WJHrBz9NauNi63hBdjQP'; // 测试用的 mint 地址
const TEST_INTERVALS = ['s1', 's30', 'm5'];

console.log('🚀 启动 K线 WebSocket 测试...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`🪙 测试代币: ${TEST_MINT}`);
console.log(`⏰ 测试间隔: ${TEST_INTERVALS.join(', ')}`);

// 创建 Socket.IO 客户端 - 连接到 /kline 命名空间
const socket = io(`${SERVER_URL}/kline`, {
    transports: ['websocket', 'polling'],
    timeout: 20000,
    reconnection: true,
    reconnectionAttempts: 5,
    reconnectionDelay: 1000,
});

// 连接事件监听
socket.on('connect', () => {
    console.log('✅ 连接成功');
    console.log(`🔌 Socket ID: ${socket.id}`);
    
    // 等待连接成功消息
    setTimeout(() => {
        startTests();
    }, 1000);
});

socket.on('disconnect', (reason) => {
    console.log(`❌ 连接断开: ${reason}`);
});

socket.on('connect_error', (error) => {
    console.log(`💥 连接错误: ${error.message}`);
});

// 接收服务器消息
socket.on('connection_success', (data) => {
    console.log('🎉 收到连接成功消息:', JSON.stringify(data, null, 2));
});

socket.on('subscription_confirmed', (data) => {
    console.log('✅ 订阅确认:', JSON.stringify(data, null, 2));
});

socket.on('unsubscribe_confirmed', (data) => {
    console.log('🚫 取消订阅确认:', JSON.stringify(data, null, 2));
});

socket.on('history_data', (data) => {
    console.log(`📈 历史数据 (${data.symbol}@${data.interval}):`, {
        symbol: data.symbol,
        interval: data.interval,
        dataPoints: data.data.length,
        hasMore: data.has_more,
        totalCount: data.total_count
    });
    
    if (data.data.length > 0) {
        console.log('   最新K线:', data.data[0]);
    }
});

socket.on('kline_data', (data) => {
    console.log(`📊 实时K线更新 (${data.symbol}@${data.interval}):`, {
        symbol: data.symbol,
        interval: data.interval,
        time: new Date(data.data.time * 1000).toISOString(),
        price: data.data.close,
        updateType: data.data.update_type,
        updateCount: data.data.update_count,
        timestamp: new Date(data.timestamp).toISOString()
    });
});

socket.on('error', (error) => {
    console.log('❌ 错误消息:', JSON.stringify(error, null, 2));
});

// 测试函数
function startTests() {
    console.log('\n🧪 开始测试流程...\n');
    
    // 测试 1: 订阅不同间隔的K线
    console.log('📊 测试 1: 订阅K线数据');
    TEST_INTERVALS.forEach((interval, index) => {
        setTimeout(() => {
            console.log(`  订阅 ${TEST_MINT}@${interval}`);
            socket.emit('subscribe', {
                symbol: TEST_MINT,
                interval: interval,
                subscription_id: `test_${interval}_${Date.now()}`
            });
        }, index * 1000);
    });
    
    // 测试 2: 获取历史数据
    setTimeout(() => {
        console.log('\n📈 测试 2: 获取历史数据');
        socket.emit('history', {
            symbol: TEST_MINT,
            interval: 's1',
            limit: 10
        });
    }, 5000);
    
    // 测试 3: 取消部分订阅
    setTimeout(() => {
        console.log('\n🚫 测试 3: 取消部分订阅');
        socket.emit('unsubscribe', {
            symbol: TEST_MINT,
            interval: 's30',
            subscription_id: `test_s30_unsubscribe`
        });
    }, 10000);
    
    // 测试 4: 重新订阅
    setTimeout(() => {
        console.log('\n🔄 测试 4: 重新订阅');
        socket.emit('subscribe', {
            symbol: TEST_MINT,
            interval: 's30',
            subscription_id: `test_s30_resubscribe_${Date.now()}`
        });
    }, 12000);
    
    // 测试 5: 压力测试 - 多个订阅
    setTimeout(() => {
        console.log('\n💪 测试 5: 压力测试 - 多个订阅');
        for (let i = 0; i < 5; i++) {
            socket.emit('subscribe', {
                symbol: `TEST_MINT_${i}`,
                interval: 's1',
                subscription_id: `stress_test_${i}_${Date.now()}`
            });
        }
    }, 15000);
    
    // 测试结束
    setTimeout(() => {
        console.log('\n🏁 测试完成，断开连接');
        socket.disconnect();
        process.exit(0);
    }, 25000);
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

console.log('📋 测试计划:');
console.log('  1. 连接到 WebSocket 服务器');
console.log('  2. 订阅多个时间间隔的K线数据');
console.log('  3. 获取历史数据');
console.log('  4. 测试取消订阅功能');
console.log('  5. 测试重新订阅功能');
console.log('  6. 压力测试多个订阅');
console.log('  7. 观察实时数据推送');
console.log('  8. 清理并断开连接\n');