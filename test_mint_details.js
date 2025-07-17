const axios = require('axios');

// Default server URL - change if needed
const SERVER_URL = 'http://localhost:8080';

async function testMintDetails() {
  try {
    console.log('🔍 Fetching available mints...');
    // First fetch available mints
    const mintsResponse = await axios.get(`${SERVER_URL}/api/mints`);
    
    if (!mintsResponse.data.success || !mintsResponse.data.data.mints.length) {
      console.error('❌ Failed to get mints or no mints available');
      return;
    }

    const mints = mintsResponse.data.data.mints;
    console.log(`✅ Found ${mints.length} mints`);

    // Take at most 5 mints for testing
    const testMints = mints.slice(0, 5);
    console.log('📊 Testing with mints:', testMints);

    // Call the details API with these mints using POST
    const detailsResponse = await axios.post(`${SERVER_URL}/api/details`, {
      mints: testMints
    });
    
    if (!detailsResponse.data.success) {
      console.error('❌ Failed to get mint details:', detailsResponse.data.error);
      return;
    }

    // Print the details
    console.log(`✅ Got details for ${detailsResponse.data.data.total} mints:`);
    detailsResponse.data.data.details.forEach(detail => {
      console.log('\n---------------------------------------');
      console.log(`🪙 Mint: ${detail.mint_account}`);
      console.log(`📛 Name: ${detail.name || 'Unknown'}`);
      console.log(`🔣 Symbol: ${detail.symbol || 'Unknown'}`);
      console.log(`🌐 URI: ${detail.uri || 'None'}`);
      
      if (detail.latest_price) {
        console.log(`💰 Latest Price: ${detail.latest_price}`);
      }
      
      if (detail.latest_trade_time) {
        const date = new Date(detail.latest_trade_time * 1000);
        console.log(`⏰ Latest Trade Time: ${date.toISOString()}`);
      }
      
      if (detail.create_timestamp) {
        const date = new Date(detail.create_timestamp * 1000);
        console.log(`🎂 Created: ${date.toISOString()}`);
      }
      
      console.log(`💵 Total SOL Amount: ${detail.total_sol_amount}`);
      console.log(`🏦 Total Margin SOL: ${detail.total_margin_sol_amount}`);
      console.log(`⚠️ Total Force Liquidations: ${detail.total_force_liquidations}`);
      console.log(`💹 Total Close Profit: ${detail.total_close_profit}`);
      console.log('---------------------------------------');
    });

  } catch (error) {
    console.error('❌ Error testing mint details:', error.message);
    if (error.response) {
      console.error('Response data:', error.response.data);
    }
  }
}

testMintDetails(); 