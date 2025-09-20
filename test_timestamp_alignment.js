#!/usr/bin/env node

// Test timestamp alignment logic
function calculateTimeBucket(timestamp, interval) {
    switch (interval) {
        case 's1':
            return timestamp; // 1-second intervals - no alignment needed
        case 's30':
            // 30-second intervals - align to 30-second boundary
            return Math.floor(timestamp / 30) * 30;
        case 'm5':
            // 5-minute intervals - align to 5-minute boundary
            return Math.floor(timestamp / 300) * 300;
        default:
            return timestamp;
    }
}

function formatTimestamp(timestamp) {
    return new Date(timestamp * 1000).toISOString();
}

console.log('🕰️ Testing timestamp alignment logic...\n');

// Test with a few example timestamps
const testTimestamps = [
    1726627853, // 2024-09-18 03:50:53
    1726627890, // 2024-09-18 03:51:30
    1726627920, // 2024-09-18 03:52:00
    1726628100, // 2024-09-18 03:55:00
    1726628220, // 2024-09-18 03:57:00
];

const intervals = ['s1', 's30', 'm5'];

testTimestamps.forEach((timestamp, i) => {
    console.log(`📊 Test ${i + 1}: ${timestamp} (${formatTimestamp(timestamp)})`);
    
    intervals.forEach(interval => {
        const aligned = calculateTimeBucket(timestamp, interval);
        console.log(`  ${interval}: ${aligned} (${formatTimestamp(aligned)})`);
    });
    
    console.log('');
});

// Test edge cases around 30-second and 5-minute boundaries
console.log('🔍 Testing boundary cases...\n');

// Test around 30-second boundary
const thirtySecBoundary = 1726627800; // 2024-09-18 03:50:00 exactly (divisible by 30)
const thirtySecTests = [
    thirtySecBoundary - 5,  // 03:49:55
    thirtySecBoundary,      // 03:50:00
    thirtySecBoundary + 15, // 03:50:15
    thirtySecBoundary + 29, // 03:50:29
    thirtySecBoundary + 30, // 03:50:30
    thirtySecBoundary + 45, // 03:50:45
    thirtySecBoundary + 59, // 03:50:59
    thirtySecBoundary + 60, // 03:51:00
];

console.log('30-second boundary tests:');
thirtySecTests.forEach(ts => {
    const s30Aligned = calculateTimeBucket(ts, 's30');
    console.log(`${ts} (${formatTimestamp(ts)}) -> s30: ${s30Aligned} (${formatTimestamp(s30Aligned)})`);
});

console.log('\n5-minute boundary tests:');
// Test around 5-minute boundary (03:50:00 is divisible by 5 minutes)
const fiveMinBoundary = 1726627800; // 2024-09-18 03:50:00 exactly
const fiveMinTests = [
    fiveMinBoundary - 60,  // 03:49:00
    fiveMinBoundary,       // 03:50:00
    fiveMinBoundary + 120, // 03:52:00
    fiveMinBoundary + 299, // 03:54:59
    fiveMinBoundary + 300, // 03:55:00
];

fiveMinTests.forEach(ts => {
    const m5Aligned = calculateTimeBucket(ts, 'm5');
    console.log(`${ts} (${formatTimestamp(ts)}) -> m5: ${m5Aligned} (${formatTimestamp(m5Aligned)})`);
});

console.log('\n✅ Timestamp alignment test completed!');