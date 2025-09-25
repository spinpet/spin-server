#!/usr/bin/env node

// 自动获取最新 mint 并监听交易事件脚本
// 先通过 API 获取最新的 mint，然后监听其交易事件数据

const { io } = require('socket.io-client');
const axios = require('axios');

// 配置
const SERVER_URL = 'http://192.168.18.5:5051';
const INTERVAL = 's30'; // K线间隔，用于订阅但不显示K线数据

let currentMint = null; 
let socket = null;

console.log('🚀 启动自动 mint 事件监听...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`⏰ 监听间隔: ${INTERVAL} (仅用于订阅，不显示K线)`);

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

// 连接 WebSocket 并监听事件
function connectAndSubscribe(mint) {
    console.log(`\n🔌 连接 WebSocket 并监听 ${mint} 的交易事件...`);
    
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
            subscribeEvents(mint);
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

    // 监听历史事件数据
    socket.on('history_event_data', (data) => {
        console.log('\n📈 收到历史事件数据:');
        console.log(`   Symbol: ${data.symbol}`);
        console.log(`   事件总数: ${data.data.length}`);
        console.log(`   Has More: ${data.has_more}`);
        console.log(`   Total Count: ${data.total_count}`);
        
        if (data.data.length > 0) {
            console.log('   最新事件:');
            const latestEvent = data.data[0];
            console.log(`     事件类型: ${latestEvent.event_type}`);
            console.log(`     时间戳: ${new Date(latestEvent.timestamp).toISOString()}`);
            console.log(`     事件详情:`, JSON.stringify(latestEvent.event_data, null, 4));
            
            // 显示前3个事件的摘要
            console.log('\n   前3个事件摘要:');
            data.data.slice(0, 3).forEach((event, index) => {
                console.log(`   ${index + 1}. 类型: ${event.event_type}, 时间: ${new Date(event.timestamp).toISOString()}`);
            });
        }
    });

    // 监听实时事件数据
    socket.on('event_data', (data) => {
        console.log('处理了 event_data');
        return
        console.log('\n🔔 收到实时事件数据:');
        console.log(`   Symbol: ${data.symbol}`);
        console.log(`   事件类型: ${data.event_type}`);
        console.log(`   时间戳: ${new Date(data.timestamp).toISOString()}`);
        console.log(`   事件详情:`, JSON.stringify(data.event_data, null, 4));
        
        // 根据事件类型显示特定信息
        switch (data.event_type) {
            case 'TokenCreated':
                console.log(`   📄 代币创建: ${data.event_data.name} (${data.event_data.symbol})`);
                break;
            case 'BuySell':
                console.log(`   💰 交易: ${data.event_data.is_buy ? '买入' : '卖出'} ${data.event_data.token_amount} tokens`);
                console.log(`   💵 价格: ${data.event_data.latest_price}`);
                break;
            case 'LongShort':
                console.log(`   📈 开仓: ${data.event_data.order_type === 1 ? '做多' : '做空'}`);
                console.log(`   💵 价格: ${data.event_data.latest_price}`);
                break;
            case 'ForceLiquidate':
                console.log(`   ⚠️ 强制平仓: ${data.event_data.order_pda}`);
                break;
            case 'FullClose':
                console.log(`   🔒 全部平仓: ${data.event_data.is_close_long ? '平多' : '平空'}`);
                console.log(`   💵 价格: ${data.event_data.latest_price}`);
                break;
            case 'PartialClose':
                console.log(`   🔓 部分平仓: ${data.event_data.is_close_long ? '平多' : '平空'}`);
                console.log(`   💵 价格: ${data.event_data.latest_price}`);
                break;
            case 'MilestoneDiscount':
                console.log(`   💲 费率调整: swap_fee=${data.event_data.swap_fee}, borrow_fee=${data.event_data.borrow_fee}`);
                break;
            default:
                console.log(`   ❓ 未知事件类型: ${data.event_type}`);
        }
    });

    // 忽略 K线数据 (静默处理)
    socket.on('history_data', (data) => {
        // 静默处理，不打印K线历史数据
    });

    socket.on('kline_data', (data) => {
        // 静默处理，不打印实时K线数据
    });

    socket.on('error', (error) => {
        console.log('❌ 错误消息:', JSON.stringify(error, null, 2));
    });

    // 捕获所有其他事件
    socket.onAny((eventName, ...args) => {
        //return
        if (!['history_data', 'kline_data', 'direct_kline_test'].includes(eventName)) {
            console.log(`🎯 收到其他事件: ${eventName}`, {
                eventName,
                argsCount: args.length,
                firstArg: args[0] ? JSON.stringify(args[0]).substring(0, 200) + '...' : 'no args'
            });
        }
    });
}

// 订阅事件数据（通过K线订阅来触发事件推送）
function subscribeEvents(mint) {
    console.log(`\n📊 订阅 ${mint} 的事件数据 (通过${INTERVAL}间隔)...`);
    socket.emit('subscribe', {
        symbol: mint,
        interval: INTERVAL,
        subscription_id: `event_monitor_${Date.now()}`
    });
    
    console.log('📈 注意: 虽然订阅了K线间隔，但我们只关心事件数据，不显示K线信息');
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
console.log('  - 订阅该 mint 的交易事件数据');
console.log('  - 显示历史事件数据（300条）');
console.log('  - 实时接收并显示交易事件更新');
console.log('  - 不显示K线数据（静默处理）');
console.log('  - 按 Ctrl+C 退出监听\n');

// 启动程序
main();