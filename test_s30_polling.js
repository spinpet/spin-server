#!/usr/bin/env node

// S30 K线轮询监听脚本
// 由于实时推送暂时有问题，使用轮询方式获取最新 S30 数据

const axios = require('axios');

// 配置
const SERVER_URL = 'http://localhost:5051';
const TEST_MINT = '7uWcH2Qviw5AAtojG97pyoAbDaN3a91pTEXMBU5cQmwx';
const INTERVAL = 's30';
const POLLING_INTERVAL = 5000; // 5秒轮询一次

console.log('🚀 启动 S30 K线轮询监听...');
console.log(`📍 服务器地址: ${SERVER_URL}`);
console.log(`🪙 代币地址: ${TEST_MINT}`);
console.log(`⏰ 监听间隔: ${INTERVAL}`);
console.log(`🔄 轮询间隔: ${POLLING_INTERVAL}ms`);

let lastKlineTime = 0;
let lastUpdateCount = 0;

async function pollKlineData() {
    try {
        const response = await axios.get(`${SERVER_URL}/api/kline`, {
            params: {
                mint: TEST_MINT,
                interval: INTERVAL,
                limit: 1
            }
        });

        if (response.data.success && response.data.data.klines.length > 0) {
            const kline = response.data.data.klines[0];
            
            // 检查是否有新的更新
            if (kline.time > lastKlineTime || 
                (kline.time === lastKlineTime && kline.update_count > lastUpdateCount)) {
                
                const klineTime = new Date(kline.time * 1000);
                const isNewKline = kline.time > lastKlineTime;
                const isUpdate = kline.time === lastKlineTime && kline.update_count > lastUpdateCount;
                
                console.log(`📊 ${isNewKline ? '新K线' : '更新'}:`, {
                    时间: klineTime.toISOString(),
                    开盘价: kline.open.toFixed(8),
                    最高价: kline.high.toFixed(8),
                    最低价: kline.low.toFixed(8),
                    收盘价: kline.close.toFixed(8),
                    成交量: kline.volume,
                    是否完结: kline.is_final ? '是' : '否',
                    更新次数: kline.update_count,
                    变化: isNewKline ? '🆕 新K线周期' : `🔄 第${kline.update_count}次更新`
                });
                
                lastKlineTime = kline.time;
                lastUpdateCount = kline.update_count;
            }
        }
    } catch (error) {
        console.error('❌ 轮询出错:', error.message);
    }
}

// 初始化：获取当前最新数据
async function initialize() {
    console.log('\n📈 获取初始数据...');
    try {
        const response = await axios.get(`${SERVER_URL}/api/kline`, {
            params: {
                mint: TEST_MINT,
                interval: INTERVAL,
                limit: 1
            }
        });

        if (response.data.success && response.data.data.klines.length > 0) {
            const kline = response.data.data.klines[0];
            lastKlineTime = kline.time;
            lastUpdateCount = kline.update_count;
            
            const klineTime = new Date(kline.time * 1000);
            console.log('✅ 初始数据获取成功:', {
                时间: klineTime.toISOString(),
                收盘价: kline.close.toFixed(8),
                更新次数: kline.update_count
            });
            console.log('\n🔄 开始监听更新...\n');
        }
    } catch (error) {
        console.error('❌ 初始化失败:', error.message);
        process.exit(1);
    }
}

// 启动监听
async function start() {
    await initialize();
    
    // 开始定期轮询
    const intervalId = setInterval(pollKlineData, POLLING_INTERVAL);
    
    // 优雅退出处理
    process.on('SIGINT', () => {
        console.log('\n👋 收到退出信号，停止监听...');
        clearInterval(intervalId);
        process.exit(0);
    });
    
    console.log('📋 功能说明:');
    console.log('  - 每5秒轮询一次最新的 S30 K线数据');
    console.log('  - 只显示有变化的数据（新K线或更新）');
    console.log('  - 按 Ctrl+C 退出监听\n');
}

// 启动应用
start().catch(error => {
    console.error('❌ 应用启动失败:', error.message);
    process.exit(1);
});