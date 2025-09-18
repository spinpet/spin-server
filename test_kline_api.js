#!/usr/bin/env node

const https = require('https');
const http = require('http');

// Helper function to make HTTP requests
function makeRequest(url, method = 'GET', data = null) {
    return new Promise((resolve, reject) => {
        const urlObj = new URL(url);
        const options = {
            hostname: urlObj.hostname,
            port: urlObj.port,
            path: urlObj.pathname + urlObj.search,
            method: method,
            headers: {
                'Content-Type': 'application/json',
            }
        };

        if (data) {
            const jsonData = JSON.stringify(data);
            options.headers['Content-Length'] = Buffer.byteLength(jsonData);
        }

        const protocol = urlObj.protocol === 'https:' ? https : http;
        const req = protocol.request(options, (res) => {
            let responseData = '';
            res.on('data', (chunk) => {
                responseData += chunk;
            });
            res.on('end', () => {
                try {
                    const parsed = JSON.parse(responseData);
                    resolve(parsed);
                } catch (e) {
                    resolve(responseData);
                }
            });
        });

        req.on('error', (err) => {
            reject(err);
        });

        if (data) {
            req.write(JSON.stringify(data));
        }
        req.end();
    });
}

async function testKlineAPI() {
    const baseUrl = 'http://localhost:5051';
    
    console.log('🧪 Testing Kline API functionality...\n');

    try {
        // Step 1: Test API endpoint with empty data
        console.log('📊 Step 1: Query empty kline data...');
        const emptyResult = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=s1&limit=5`);
        console.log('Empty result:', JSON.stringify(emptyResult, null, 2));
        console.log('✅ Empty query successful\n');

        // Step 2: Create some test token data
        console.log('🪙 Step 2: Create test token to generate kline data...');
        const testTokenData = {
            mint_account: "test_mint_12345",
            uri: "https://example.com/test.json",
            name: "Test Token",
            symbol: "TEST",
            payer: "test_payer_12345"
        };
        
        const createResult = await makeRequest(`${baseUrl}/api/test-ipfs`, 'POST', testTokenData);
        console.log('Token creation result:', JSON.stringify(createResult, null, 2));
        console.log('✅ Test token created\n');

        // Wait a moment for processing
        await new Promise(resolve => setTimeout(resolve, 1000));

        // Step 3: Test different intervals
        console.log('📈 Step 3: Test different kline intervals...');
        
        const intervals = ['s1', 'm1', 'm5'];
        for (const interval of intervals) {
            console.log(`\n🔍 Testing interval: ${interval}`);
            const result = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=${interval}&limit=10&order_by=time_desc`);
            console.log(`${interval} result:`, JSON.stringify(result, null, 2));
        }

        // Step 4: Test pagination
        console.log('\n📑 Step 4: Test pagination...');
        const paginationResult = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=s1&page=1&limit=5`);
        console.log('Pagination result:', JSON.stringify(paginationResult, null, 2));

        // Step 5: Test sorting
        console.log('\n⬆️ Step 5: Test ascending sort...');
        const ascResult = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=s1&order_by=time_asc&limit=5`);
        console.log('Ascending sort result:', JSON.stringify(ascResult, null, 2));

        // Step 6: Test error cases
        console.log('\n❌ Step 6: Test error cases...');
        
        // Invalid interval
        console.log('Testing invalid interval...');
        const invalidInterval = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=invalid&limit=5`);
        console.log('Invalid interval result:', JSON.stringify(invalidInterval, null, 2));
        
        // Empty mint
        console.log('\nTesting empty mint...');
        const emptyMint = await makeRequest(`${baseUrl}/api/kline?mint=&interval=s1&limit=5`);
        console.log('Empty mint result:', JSON.stringify(emptyMint, null, 2));
        
        // Invalid order_by
        console.log('\nTesting invalid order_by...');
        const invalidOrder = await makeRequest(`${baseUrl}/api/kline?mint=test_mint_12345&interval=s1&order_by=invalid&limit=5`);
        console.log('Invalid order_by result:', JSON.stringify(invalidOrder, null, 2));

        console.log('\n🎉 Kline API testing completed!');

    } catch (error) {
        console.error('❌ Test failed:', error.message);
    }
}

// Run the test
testKlineAPI().catch(console.error);