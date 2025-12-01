#!/usr/bin/env bash
# Fail fast on any error, unset var, or failed pipe; show commands for easier debugging
set -euo pipefail

# Build the shared library
echo "Building shared library..."
cd shared
cargo build --release
cd ..

# Build the invitation service
echo "Building invitation service..."
cd invitation-service
cargo build --release --target x86_64-unknown-linux-musl
cd ..

# Build the box service
echo "Building box service..."
cd box-service
cargo build --release --target x86_64-unknown-linux-musl
cd ..

# Build the invitation event handler
echo "Building invitation event handler..."
cd invitation-event-service
cargo build --release --target x86_64-unknown-linux-musl
cd ..

# Build the notification service
echo "Building notification service..."
cd notification-service
cargo build --release --target x86_64-unknown-linux-musl
cd ..

# Package the invitation service
echo "Packaging invitation service..."
mkdir -p dist
cp invitation-service/target/x86_64-unknown-linux-musl/release/lockbox-invitation-service ./bootstrap
zip -j invitation-service.zip bootstrap
rm bootstrap

# Package the box service
echo "Packaging box service..."
cp box-service/target/x86_64-unknown-linux-musl/release/lockbox-box-service ./bootstrap
zip -j box-service.zip bootstrap
rm bootstrap

# Package the invitation event handler
echo "Packaging invitation event handler..."
cp invitation-event-service/target/x86_64-unknown-linux-musl/release/invitation-event-service ./bootstrap
zip -j invitation-event-handler.zip bootstrap
rm bootstrap

# Package the notification service
echo "Packaging notification service..."
cp notification-service/target/x86_64-unknown-linux-musl/release/notification-service ./bootstrap
zip -j notification-service.zip bootstrap
rm bootstrap

echo "Build process complete! Lambda zip files are ready for deployment." 