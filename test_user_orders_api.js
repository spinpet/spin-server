const axios = require('axios');

const BASE_URL = 'http://localhost:3000';

async function testUserOrdersAPI() {
    console.log('🧪 Testing User Orders API...\n');

    try {
        // Test 1: Query user orders with default parameters
        console.log('1. Testing user orders query with default parameters...');
        const response1 = await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123`);
        console.log('✅ Response:', {
            status: response1.status,
            total: response1.data.data.total,
            page: response1.data.data.page,
            limit: response1.data.data.limit,
            has_next: response1.data.data.has_next,
            has_prev: response1.data.data.has_prev,
            orders_count: response1.data.data.orders.length,
            mint_account: response1.data.data.mint_account
        });
        console.log('📋 Sample order (if any):', response1.data.data.orders[0] || 'No orders found');
        console.log('');

        // Test 1.5: Query user orders with mint parameter
        console.log('1.5. Testing user orders query with mint parameter...');
        const response1_5 = await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&mint=test_mint_456`);
        console.log('✅ Response:', {
            status: response1_5.status,
            total: response1_5.data.data.total,
            mint_account: response1_5.data.data.mint_account,
            orders_count: response1_5.data.data.orders.length
        });
        console.log('📋 Sample order (if any):', response1_5.data.data.orders[0] || 'No orders found');
        console.log('');

        // Test 2: Query user orders with pagination
        console.log('2. Testing user orders query with pagination...');
        const response2 = await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&page=1&limit=10`);
        console.log('✅ Response:', {
            status: response2.status,
            total: response2.data.data.total,
            page: response2.data.data.page,
            limit: response2.data.data.limit
        });
        console.log('');

        // Test 3: Query user orders with ascending order
        console.log('3. Testing user orders query with ascending order...');
        const response3 = await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&order_by=start_time_asc`);
        console.log('✅ Response:', {
            status: response3.status,
            total: response3.data.data.total,
            order_by: 'start_time_asc'
        });
        console.log('');

        // Test 4: Query user orders with descending order (default)
        console.log('4. Testing user orders query with descending order (default)...');
        const response4 = await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&order_by=start_time_desc`);
        console.log('✅ Response:', {
            status: response4.status,
            total: response4.data.data.total,
            order_by: 'start_time_desc'
        });
        console.log('');

        // Test 5: Test error handling - empty user
        console.log('5. Testing error handling - empty user...');
        try {
            await axios.get(`${BASE_URL}/api/user_orders?user=`);
        } catch (error) {
            console.log('✅ Expected error caught:', error.response.data.message);
        }
        console.log('');

        // Test 6: Test error handling - invalid order_by
        console.log('6. Testing error handling - invalid order_by...');
        try {
            await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&order_by=invalid`);
        } catch (error) {
            console.log('✅ Expected error caught:', error.response.data.message);
        }
        console.log('');

        // Test 7: Test error handling - limit too high
        console.log('7. Testing error handling - limit too high...');
        try {
            await axios.get(`${BASE_URL}/api/user_orders?user=test_user_123&limit=1001`);
        } catch (error) {
            console.log('✅ Expected error caught:', error.response.data.message);
        }
        console.log('');

        console.log('🎉 All tests completed successfully!');

    } catch (error) {
        console.error('❌ Test failed:', error.message);
        if (error.response) {
            console.error('Response data:', error.response.data);
            console.error('Response status:', error.response.status);
        }
    }
}

// Run the tests
testUserOrdersAPI();
