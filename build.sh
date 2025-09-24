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

# Build the box invitation handler
echo "Building box invitation handler..."
cd box-invitation-handler
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

# Package the box invitation handler
echo "Packaging box invitation handler..."
cp box-invitation-handler/target/x86_64-unknown-linux-musl/release/lockbox-box-invitation-handler ./bootstrap
zip -j box-invitation-handler.zip bootstrap
rm bootstrap

echo "Build process complete! Lambda zip files are ready for deployment." 