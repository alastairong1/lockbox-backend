#!/usr/bin/env bash
# Fail fast on any error, unset var, or failed pipe; show commands for easier debugging
set -euo pipefail

# First run tests with mock store (default)
echo "=== Running unit tests with mock store ==="

# Run invitation service tests
echo "Running invitation service unit tests..."
pushd invitation-service
cargo test -- --test-threads=1
popd

# Run box service tests
echo "Running box service unit tests..."
pushd box-service
cargo test -- --test-threads=1
popd

# Run guardian service tests if available
if [ -d "guardian-service" ]; then
  echo "Running guardian service unit tests..."
  pushd guardian-service
  cargo test -- --test-threads=1
  popd
fi

# Run invitation event service tests
echo "Running invitation event service unit tests..."
pushd invitation-event-service
cargo test -- --test-threads=1
popd

# Run notification service tests
echo "Running notification service unit tests..."
pushd notification-service
cargo test -- --test-threads=1
popd

# Run reminder service tests
echo "Running reminder service unit tests..."
pushd reminder-service
cargo test -- --test-threads=1
popd

# Then run integration tests with DynamoDB Local
echo ""
echo "=== Running integration tests with DynamoDB store ==="
# Call the integration tests script
./run-integration-tests.sh

echo ""
echo "All tests completed successfully!" 