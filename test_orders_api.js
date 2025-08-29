// Test script for order API with sorting and pagination
const fetch = require('node-fetch');

// Configuration
const API_URL = 'http://localhost:8080';
const MINT_ADDRESS = process.argv[2] || '3TcTZaiCMhCDF2PM7QBzX2aHFeJqLKJrd9LFGLugkr5x'; // Default mint or from command line

// Test functions
async function testUpOrders() {
  console.log('Testing up_orders with pagination and sorting...');
  
  // Test page 1 with limit 5
  const response1 = await fetch(`${API_URL}/api/mint_orders?mint=${MINT_ADDRESS}&type=up_orders&page=1&limit=5`);
  const data1 = await response1.json();
  
  console.log(`Page 1 (limit 5) - Total: ${data1.data.total}, Retrieved: ${data1.data.orders.length}`);
  console.log('Has next page:', data1.data.has_next);
  console.log('Has previous page:', data1.data.has_prev);
  
  if (data1.data.orders.length > 0) {
    console.log('First few prices (should be sorted ascending):');
    data1.data.orders.forEach((order, i) => {
      console.log(`  ${i + 1}. lock_lp_start_price: ${order.lock_lp_start_price}`);
    });
    
    // Test page 2 if available
    if (data1.data.has_next) {
      const response2 = await fetch(`${API_URL}/api/mint_orders?mint=${MINT_ADDRESS}&type=up_orders&page=2&limit=5`);
      const data2 = await response2.json();
      
      console.log('\nPage 2 (limit 5) - Total:', data2.data.total);
      console.log('Has next page:', data2.data.has_next);
      console.log('Has previous page:', data2.data.has_prev);
      
      console.log('First few prices (should be sorted ascending and greater than page 1):');
      data2.data.orders.forEach((order, i) => {
        console.log(`  ${i + 1}. lock_lp_start_price: ${order.lock_lp_start_price}`);
      });
    }
  }
}

async function testDownOrders() {
  console.log('\nTesting down_orders with pagination and sorting...');
  
  // Test page 1 with limit 5
  const response = await fetch(`${API_URL}/api/mint_orders?mint=${MINT_ADDRESS}&type=down_orders&page=1&limit=5`);
  const data = await response.json();
  
  console.log(`Page 1 (limit 5) - Total: ${data.data.total}, Retrieved: ${data.data.orders.length}`);
  console.log('Has next page:', data.data.has_next);
  console.log('Has previous page:', data.data.has_prev);
  
  if (data.data.orders.length > 0) {
    console.log('First few prices (should be sorted descending):');
    data.data.orders.forEach((order, i) => {
      console.log(`  ${i + 1}. lock_lp_start_price: ${order.lock_lp_start_price}`);
    });
    
    // Test page 2 if available
    if (data.data.has_next) {
      const response2 = await fetch(`${API_URL}/api/mint_orders?mint=${MINT_ADDRESS}&type=down_orders&page=2&limit=5`);
      const data2 = await response2.json();
      
      console.log('\nPage 2 (limit 5) - Total:', data2.data.total);
      console.log('Has next page:', data2.data.has_next);
      console.log('Has previous page:', data2.data.has_prev);
      
      console.log('First few prices (should be sorted descending and less than page 1):');
      data2.data.orders.forEach((order, i) => {
        console.log(`  ${i + 1}. lock_lp_start_price: ${order.lock_lp_start_price}`);
      });
    }
  }
}

// Run tests
async function runTests() {
  try {
    await testUpOrders();
    await testDownOrders();
    console.log('\nTests completed successfully!');
  } catch (error) {
    console.error('Error during testing:', error);
  }
}

runTests(); 