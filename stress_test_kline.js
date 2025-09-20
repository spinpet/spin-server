#!/usr/bin/env node

// K线 WebSocket 压力测试脚本
// 测试高并发连接和订阅场景

const { io } = require('socket.io-client');

// 测试配置
const SERVER_URL = 'http://localhost:5051';
const NUM_CLIENTS = 20;                    // 并发客户端数量
const SUBSCRIPTIONS_PER_CLIENT = 5;       // 每客户端订阅数
const TEST_DURATION = 30000;              // 测试持续时间（毫秒）
const CONNECTION_DELAY = 100;             // 连接间隔（毫秒）

console.log('🚀 启动 K线 WebSocket 压力测试...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`👥 并发客户端: ${NUM_CLIENTS}`);
console.log(`📊 每客户端订阅数: ${SUBSCRIPTIONS_PER_CLIENT}`);
console.log(`⏱️ 测试持续时间: ${TEST_DURATION / 1000}秒`);
console.log(`🔗 连接间隔: ${CONNECTION_DELAY}毫秒\n`);

// 统计信息
const stats = {
    connectedClients: 0,
    failedConnections: 0,
    totalSubscriptions: 0,
    totalMessages: 0,
    totalErrors: 0,
    messagesPerSecond: 0,
    startTime: Date.now(),
    lastSecondMessages: 0,
    lastSecondTime: Date.now(),
};

// 生成测试数据
function generateTestData() {
    const mintAccounts = [];
    const intervals = ['s1', 's30', 'm5'];
    
    // 生成一些模拟的 mint 地址
    for (let i = 0; i < 100; i++) {
        mintAccounts.push(`MINT${i.toString().padStart(8, '0')}${'1'.repeat(32)}`);
    }
    
    return { mintAccounts, intervals };
}

// 创建客户端
function createClient(clientId) {
    return new Promise((resolve, reject) => {
        console.log(`🔌 创建客户端 ${clientId}...`);
        
        const socket = io(`${SERVER_URL}/kline`, {
            transports: ['websocket', 'polling'],
            timeout: 10000,
            reconnection: false, // 关闭自动重连以避免测试混乱
        });
        
        let connectionEstablished = false;
        
        // 连接超时处理
        const connectionTimeout = setTimeout(() => {
            if (!connectionEstablished) {
                console.log(`❌ 客户端 ${clientId} 连接超时`);
                stats.failedConnections++;
                socket.disconnect();
                reject(new Error('Connection timeout'));
            }
        }, 10000);
        
        socket.on('connect', () => {
            connectionEstablished = true;
            clearTimeout(connectionTimeout);
            stats.connectedClients++;
            console.log(`✅ 客户端 ${clientId} 连接成功 (Socket ID: ${socket.id})`);
            resolve(socket);
        });
        
        socket.on('connect_error', (error) => {
            clearTimeout(connectionTimeout);
            console.log(`💥 客户端 ${clientId} 连接失败: ${error.message}`);
            stats.failedConnections++;
            reject(error);
        });
        
        socket.on('disconnect', (reason) => {
            console.log(`🔌 客户端 ${clientId} 断开连接: ${reason}`);
            if (stats.connectedClients > 0) {
                stats.connectedClients--;
            }
        });
        
        // 监听消息
        socket.on('connection_success', () => {
            stats.totalMessages++;
        });
        
        socket.on('subscription_confirmed', () => {
            stats.totalMessages++;
        });
        
        socket.on('history_data', () => {
            stats.totalMessages++;
        });
        
        socket.on('kline_data', () => {
            stats.totalMessages++;
            stats.lastSecondMessages++;
        });
        
        socket.on('error', () => {
            stats.totalErrors++;
        });
    });
}

// 为客户端创建订阅
async function createSubscriptions(socket, clientId) {
    const { mintAccounts, intervals } = generateTestData();
    const subscriptions = [];
    
    for (let i = 0; i < SUBSCRIPTIONS_PER_CLIENT; i++) {
        const mint = mintAccounts[Math.floor(Math.random() * mintAccounts.length)];
        const interval = intervals[Math.floor(Math.random() * intervals.length)];
        
        const subscription = {
            symbol: mint,
            interval: interval,
            subscription_id: `client${clientId}_sub${i}_${Date.now()}`
        };
        
        subscriptions.push(subscription);
        
        try {
            socket.emit('subscribe', subscription);
            stats.totalSubscriptions++;
            console.log(`📊 客户端 ${clientId} 订阅: ${mint.slice(0, 8)}...@${interval}`);
            
            // 稍微延迟以避免过载
            await new Promise(resolve => setTimeout(resolve, 50));
        } catch (error) {
            console.log(`❌ 客户端 ${clientId} 订阅失败: ${error.message}`);
            stats.totalErrors++;
        }
    }
    
    return subscriptions;
}

// 统计更新函数
function updateStats() {
    const now = Date.now();
    const timeDiff = now - stats.lastSecondTime;
    
    if (timeDiff >= 1000) {
        stats.messagesPerSecond = Math.round((stats.lastSecondMessages * 1000) / timeDiff);
        stats.lastSecondMessages = 0;
        stats.lastSecondTime = now;
    }
}

// 打印实时统计信息
function printStats() {
    const uptime = (Date.now() - stats.startTime) / 1000;
    updateStats();
    
    console.log('\n📊 实时统计:');
    console.log(`  连接的客户端: ${stats.connectedClients}/${NUM_CLIENTS}`);
    console.log(`  失败的连接: ${stats.failedConnections}`);
    console.log(`  总订阅数: ${stats.totalSubscriptions}`);
    console.log(`  总消息数: ${stats.totalMessages}`);
    console.log(`  错误数: ${stats.totalErrors}`);
    console.log(`  消息/秒: ${stats.messagesPerSecond}`);
    console.log(`  运行时间: ${uptime.toFixed(1)}秒`);
    console.log('─'.repeat(50));
}

// 主测试函数
async function runStressTest() {
    const clients = [];
    const clientPromises = [];
    
    console.log('\n🚀 开始创建客户端连接...\n');
    
    // 创建客户端（带间隔以避免过载）
    for (let i = 0; i < NUM_CLIENTS; i++) {
        const promise = createClient(i).then(async (socket) => {
            // 等待一会儿再创建订阅
            await new Promise(resolve => setTimeout(resolve, 1000));
            await createSubscriptions(socket, i);
            return socket;
        }).catch(error => {
            console.log(`❌ 客户端 ${i} 初始化失败: ${error.message}`);
            return null;
        });
        
        clientPromises.push(promise);
        
        // 延迟创建下一个客户端
        if (i < NUM_CLIENTS - 1) {
            await new Promise(resolve => setTimeout(resolve, CONNECTION_DELAY));
        }
    }
    
    // 等待所有客户端连接完成
    console.log('\n⏳ 等待所有客户端连接...\n');
    const connectedSockets = await Promise.all(clientPromises);
    const validSockets = connectedSockets.filter(socket => socket !== null);
    
    console.log(`\n✅ ${validSockets.length}/${NUM_CLIENTS} 客户端成功连接`);
    console.log(`📊 总订阅数: ${stats.totalSubscriptions}`);
    
    // 开始统计监控
    const statsInterval = setInterval(printStats, 2000);
    
    // 运行测试指定时间
    console.log(`\n🏃 开始 ${TEST_DURATION / 1000} 秒压力测试...\n`);
    
    await new Promise(resolve => setTimeout(resolve, TEST_DURATION));
    
    // 清理
    clearInterval(statsInterval);
    
    console.log('\n🧹 正在清理连接...');
    validSockets.forEach((socket, index) => {
        if (socket) {
            socket.disconnect();
        }
    });
    
    // 最终统计
    console.log('\n🏁 压力测试完成!\n');
    printStats();
    
    const totalTime = (Date.now() - stats.startTime) / 1000;
    const avgMessagesPerSecond = stats.totalMessages / totalTime;
    
    console.log('\n📈 最终结果:');
    console.log(`  测试持续时间: ${totalTime.toFixed(1)}秒`);
    console.log(`  成功连接率: ${((NUM_CLIENTS - stats.failedConnections) / NUM_CLIENTS * 100).toFixed(1)}%`);
    console.log(`  平均消息/秒: ${avgMessagesPerSecond.toFixed(1)}`);
    console.log(`  错误率: ${(stats.totalErrors / (stats.totalMessages + stats.totalErrors) * 100).toFixed(2)}%`);
    
    if (avgMessagesPerSecond > 50) {
        console.log('🎉 性能表现优秀! (>50 消息/秒)');
    } else if (avgMessagesPerSecond > 20) {
        console.log('👍 性能表现良好! (>20 消息/秒)');
    } else {
        console.log('⚠️ 性能需要优化 (<20 消息/秒)');
    }
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
    console.log('\n👋 收到退出信号，正在停止测试...');
    process.exit(0);
});

// 运行测试
runStressTest().then(() => {
    console.log('\n✅ 测试完成，退出程序');
    process.exit(0);
}).catch(error => {
    console.error('\n❌ 测试失败:', error);
    process.exit(1);
});