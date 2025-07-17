const WebSocket = require('ws');

console.log('Testing WebSocket connection...');

// Test different WebSocket endpoints
const endpoints = [
    'ws://127.0.0.1:8900',
    'ws://127.0.0.1:8900/',
    'ws://127.0.0.1:8899',
    'ws://127.0.0.1:8899/',
];

async function testWebSocket(url) {
    return new Promise((resolve) => {
        console.log(`\nTesting: ${url}`);
        const ws = new WebSocket(url);
        
        const timeout = setTimeout(() => {
            ws.close();
            resolve({ url, status: 'timeout' });
        }, 3000);
        
        ws.on('open', () => {
            console.log(`✅ ${url} - Connection successful`);
            clearTimeout(timeout);
            
            // Send subscription request
            const subscribeRequest = {
                jsonrpc: "2.0",
                id: "test",
                method: "logsSubscribe",
                params: [
                    {
                        mentions: ["2uA6nkQRmnPTPWuw73pStTQRWnX15BkzV6zSsEG1nKyC"]
                    },
                    {
                        commitment: "confirmed"
                    }
                ]
            };
            
            ws.send(JSON.stringify(subscribeRequest));
            
            setTimeout(() => {
                ws.close();
                resolve({ url, status: 'success' });
            }, 1000);
        });
        
        ws.on('error', (error) => {
            console.log(`❌ ${url} - Connection failed: ${error.message}`);
            clearTimeout(timeout);
            resolve({ url, status: 'error', error: error.message });
        });
        
        ws.on('message', (data) => {
            console.log(`📨 ${url} - Received message: ${data}`);
        });
    });
}

async function main() {
    console.log('Starting WebSocket endpoint tests...\n');
    
    for (const endpoint of endpoints) {
        await testWebSocket(endpoint);
        await new Promise(resolve => setTimeout(resolve, 500));
    }
    
    console.log('\nTesting complete!');
}

main().catch(console.error); 