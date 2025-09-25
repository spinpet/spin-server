#!/usr/bin/env node

// 自动获取最新 mint 并监听 K线数据脚本
// 先通过 API 获取最新的 mint，然后监听其行情数据

const { io } = require('socket.io-client');
const axios = require('axios');

// 配置
const SERVER_URL = 'http://192.168.18.5:5051';
const INTERVAL = 's30';

let currentMint = null; 
let socket = null;

console.log('🚀 启动自动 mint 监听...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`⏰ 监听间隔: ${INTERVAL}`);

// 获取最新的 mint 地址
async function getLatestMint() {
    try {
        console.log('\n📡 正在获取最新的 mint 地址...');
        
        const response = await axios.get(`${SERVER_URL}/api/mints`, {
            headers: {
                'accept': 'application/json'
            }
        });
        
        if (response.data.success && response.data.data.mints.length > 0) {
            const latestMint = response.data.data.mints[0];
            console.log(`✅ 获取到最新 mint: ${latestMint}`);
            console.log(`📊 总共找到 ${response.data.data.mints.length} 个 mint`);
            return latestMint;
        } else {
            throw new Error('未找到可用的 mint 地址');
        }
    } catch (error) {
        console.error('❌ 获取 mint 地址失败:', error.message);
        if (error.response) {
            console.error('响应状态:', error.response.status);
            console.error('响应数据:', error.response.data);
        }
        return null;
    }
}

// 连接 WebSocket 并监听行情
function connectAndSubscribe(mint) {
    console.log(`\n🔌 连接 WebSocket 并监听 ${mint} 的行情...`);
    
    // 创建 Socket.IO 客户端 - 连接到 /kline 命名空间
    socket = io(`${SERVER_URL}/kline`, {
        transports: ['websocket', 'polling'],
        timeout: 20000,
        reconnection: true,
        reconnectionAttempts: 10,
        reconnectionDelay: 2000,
    });

    // 连接事件监听
    socket.on('connect', () => {
        console.log('✅ WebSocket 连接成功');
        console.log(`🔌 Socket ID: ${socket.id}`);
        
        // 等待连接稳定后订阅
        setTimeout(() => {
            subscribeKline(mint);
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
        console.log('✅ 订阅确认:', JSON.stringify(data, null, 2));
    });

    socket.on('history_data', (data) => {
        if (data.interval === INTERVAL) {
            console.log(`📈 历史数据:`, {
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
        console.log(`🔔 收到K线数据 (原始):`, {
            interval: data.interval,
            expected: INTERVAL,
            symbol: data.symbol,
            timestamp: data.timestamp,
            dataSize: JSON.stringify(data).length
        });
        
        if (data.interval === INTERVAL) {
            const klineTime = new Date(data.data.time * 1000);
            console.log(`📊 实时K线更新:`, {
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
        } else {
            console.log(`⚠️ 收到其他间隔的K线数据: ${data.interval}, 期望: ${INTERVAL}`);
        }
    });

    socket.on('error', (error) => {
        console.log('❌ 错误消息:', JSON.stringify(error, null, 2));
    });

    // 监听直接测试事件
    socket.on('direct_kline_test', (data) => {
        console.log('🧪 收到直接测试消息:', {
            interval: data.interval,
            symbol: data.symbol,
            timestamp: new Date(data.timestamp).toISOString()
        });
    });

    // 捕获所有事件
    socket.onAny((eventName, ...args) => {
        console.log(`🎯 收到事件: ${eventName}`, {
            eventName,
            argsCount: args.length,
            firstArg: args[0] ? JSON.stringify(args[0]).substring(0, 200) + '...' : 'no args'
        });
    });
}

// 订阅 K线数据
function subscribeKline(mint) {
    console.log(`\n📊 订阅 ${mint} 的 ${INTERVAL} K线数据...`);
    socket.emit('subscribe', {
        symbol: mint,
        interval: INTERVAL,
        subscription_id: `auto_monitor_${Date.now()}`
    });
    
    // 获取一些历史数据作为参考
    setTimeout(() => {
        console.log('📈 获取最近10条历史数据...');
        socket.emit('history', {
            symbol: mint,
            interval: INTERVAL,
            limit: 10
        });
    }, 2000);
}

// 主函数
async function main() {
    try {
        // 1. 获取最新的 mint
        const latestMint = await getLatestMint();
        if (!latestMint) {
            console.error('❌ 无法获取 mint 地址，退出程序');
            process.exit(1);
        }
        
        currentMint = latestMint;
        
        // 2. 连接 WebSocket 并开始监听
        connectAndSubscribe(latestMint);
        
    } catch (error) {
        console.error('❌ 程序运行出错:', error.message);
        process.exit(1);
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
    console.log('\n👋 收到退出信号，正在断开连接...');
    if (socket) {
        socket.disconnect();
    }
    process.exit(0);
});

console.log('\n📋 功能说明:');
console.log('  - 通过 API 自动获取最新的 mint 地址');
console.log('  - 连接到 WebSocket 服务器');
console.log('  - 订阅该 mint 的 K线数据');
console.log('  - 持续接收并显示实时更新');
console.log('  - 按 Ctrl+C 退出监听\n');

// 启动程序
main();