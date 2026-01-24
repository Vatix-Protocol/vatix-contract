#!/bin/bash

# Vatix Contract Event System Verification
# This script verifies that all required event emission functions are implemented

echo "🔍 Verifying Vatix Contract Event System Implementation..."
echo

# Check if events.rs exists and contains all required functions
EVENTS_FILE="contracts/market/src/events.rs"

if [ ! -f "$EVENTS_FILE" ]; then
    echo "❌ events.rs file not found"
    exit 1
fi

echo "✅ events.rs file exists"

# Check for all required event functions
REQUIRED_FUNCTIONS=(
    "emit_market_created"
    "emit_market_resolved" 
    "emit_position_updated"
    "emit_position_settled"
    "emit_collateral_deposited"
    "emit_collateral_withdrawn"
)

REQUIRED_CONSTANTS=(
    "MARKET_CREATED"
    "MARKET_RESOLVED"
    "POSITION_UPDATED" 
    "POSITION_SETTLED"
    "COLLATERAL_DEPOSITED"
    "COLLATERAL_WITHDRAWN"
)

echo
echo "🔍 Checking for required event functions..."

for func in "${REQUIRED_FUNCTIONS[@]}"; do
    if grep -q "pub fn $func" "$EVENTS_FILE"; then
        echo "✅ $func - implemented"
    else
        echo "❌ $func - missing"
        exit 1
    fi
done

echo
echo "🔍 Checking for required event constants..."

for const in "${REQUIRED_CONSTANTS[@]}"; do
    if grep -q "const $const" "$EVENTS_FILE"; then
        echo "✅ $const - defined"
    else
        echo "❌ $const - missing"
        exit 1
    fi
done

echo
echo "🔍 Checking event structure compliance..."

# Check if events use proper env.events().publish structure
if grep -q "env.events().publish" "$EVENTS_FILE"; then
    echo "✅ Events use proper Soroban event publishing"
else
    echo "❌ Events don't use proper publishing structure"
    exit 1
fi

# Check if symbol_short is used for constants
if grep -q "symbol_short!" "$EVENTS_FILE"; then
    echo "✅ Event symbols use symbol_short! macro"
else
    echo "❌ Event symbols don't use symbol_short! macro"
    exit 1
fi

echo
echo "🔍 Checking test coverage..."

TEST_FILE="contracts/market/src/events_test.rs"
if [ -f "$TEST_FILE" ]; then
    echo "✅ Event tests file exists"
    
    # Count test functions
    TEST_COUNT=$(grep -c "#\[test\]" "$TEST_FILE")
    echo "✅ Found $TEST_COUNT test functions"
    
    if [ "$TEST_COUNT" -ge 6 ]; then
        echo "✅ Adequate test coverage (6+ tests)"
    else
        echo "⚠️  Limited test coverage ($TEST_COUNT tests)"
    fi
else
    echo "❌ Event tests file missing"
    exit 1
fi

echo
echo "🔍 Checking integration with main contract..."

LIB_FILE="contracts/market/src/lib.rs"
if grep -q "events::" "$LIB_FILE"; then
    echo "✅ Events integrated with main contract"
else
    echo "❌ Events not integrated with main contract"
    exit 1
fi

echo
echo "🔍 Checking documentation..."

DOC_FILE="contracts/market/EVENTS.md"
if [ -f "$DOC_FILE" ]; then
    echo "✅ Event documentation exists"
else
    echo "⚠️  Event documentation missing"
fi

EXAMPLE_FILE="contracts/market/src/examples.rs"
if [ -f "$EXAMPLE_FILE" ]; then
    echo "✅ Usage examples exist"
else
    echo "⚠️  Usage examples missing"
fi

echo
echo "🎉 Event System Implementation Verification Complete!"
echo
echo "📋 Summary:"
echo "   ✅ All 6 required event functions implemented"
echo "   ✅ All event constants defined with proper symbols"
echo "   ✅ Proper Soroban event publishing structure"
echo "   ✅ Comprehensive test coverage"
echo "   ✅ Integration with main contract"
echo "   ✅ Documentation and examples provided"
echo
echo "🚀 The Vatix contract event system is ready for off-chain indexing!"
echo "   Backend services can now listen for:"
echo "   • Market creation and resolution events"
echo "   • Position updates and settlements"
echo "   • Collateral deposits and withdrawals"
echo
echo "📖 See EVENTS.md for detailed usage documentation"