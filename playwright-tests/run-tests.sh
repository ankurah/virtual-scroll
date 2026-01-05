#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    pkill -f "vite" 2>/dev/null || true
    pkill -f "node.*5199" 2>/dev/null || true
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Build WASM if needed
if [ "$1" = "--build" ] || [ ! -d "wasm-bindings/pkg" ]; then
    echo -e "${YELLOW}Building WASM bindings...${NC}"
    cd wasm-bindings
    wasm-pack build --target web --dev
    cd ..
fi

# Install dependencies if needed
if [ ! -d "react-app/node_modules" ]; then
    echo -e "${YELLOW}Installing react-app dependencies...${NC}"
    cd react-app
    npm install
    cd ..
fi

if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}Installing playwright dependencies...${NC}"
    npm install
fi

# Kill any existing server
cleanup

# Start dev server in background
echo -e "${YELLOW}Starting dev server...${NC}"
cd react-app
npm run dev &
DEV_PID=$!
cd ..

# Wait for server to be ready
echo -e "${YELLOW}Waiting for server to be ready...${NC}"
MAX_WAIT=30
WAIT_COUNT=0
while ! curl -s http://localhost:5199/ > /dev/null 2>&1; do
    sleep 1
    WAIT_COUNT=$((WAIT_COUNT + 1))
    if [ $WAIT_COUNT -ge $MAX_WAIT ]; then
        echo -e "${RED}Server failed to start within ${MAX_WAIT} seconds${NC}"
        exit 1
    fi
done
echo -e "${GREEN}Server is ready!${NC}"

# Run playwright tests
echo -e "${YELLOW}Running Playwright tests...${NC}"
# All remaining args after --build (if present) go to playwright
ARGS=()
SKIP_NEXT=false
for arg in "$@"; do
    if [ "$SKIP_NEXT" = true ]; then
        SKIP_NEXT=false
        continue
    fi
    if [ "$arg" = "--build" ]; then
        continue
    fi
    ARGS+=("$arg")
done

# Use line reporter for cleaner output, show last 10 failures
if [ ${#ARGS[@]} -eq 0 ]; then
    npx playwright test --reporter=line 2>&1 | tee /tmp/playwright-output.txt
else
    npx playwright test --reporter=line "${ARGS[@]}" 2>&1 | tee /tmp/playwright-output.txt
fi

TEST_EXIT_CODE=${PIPESTATUS[0]}

if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}All tests passed!${NC}"
else
    echo -e "${RED}Some tests failed${NC}"
    echo ""
    echo -e "${YELLOW}First 5 errors:${NC}"
    grep -A 10 "^  [0-9]*)" /tmp/playwright-output.txt | head -60
    echo ""
    echo -e "${YELLOW}Error context files:${NC}"
    find test-results -name "error-context.md" 2>/dev/null | head -3 | while read f; do
        echo "--- $f ---"
        cat "$f"
        echo ""
    done
fi

exit $TEST_EXIT_CODE
