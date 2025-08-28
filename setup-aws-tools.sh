#!/bin/bash

# Script to install AWS CLI and SAM CLI

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}     AWS Tools Setup Script${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""

# Detect OS
OS="unknown"
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS="macos"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    OS="linux"
else
    echo -e "${RED}Unsupported OS: $OSTYPE${NC}"
    exit 1
fi

echo -e "Detected OS: ${YELLOW}${OS}${NC}"
echo ""

# Function to install on macOS
install_macos() {
    # Check for Homebrew
    if ! command -v brew &> /dev/null; then
        echo -e "${YELLOW}Homebrew not found. Installing Homebrew...${NC}"
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
    fi
    
    # Install AWS CLI
    if ! command -v aws &> /dev/null; then
        echo -e "${YELLOW}Installing AWS CLI...${NC}"
        brew install awscli
    else
        echo -e "${GREEN}✓ AWS CLI already installed${NC}"
    fi
    
    # Install SAM CLI
    if ! command -v sam &> /dev/null; then
        echo -e "${YELLOW}Installing SAM CLI...${NC}"
        brew install aws-sam-cli
    else
        echo -e "${GREEN}✓ SAM CLI already installed${NC}"
    fi
    
    # Install jq
    if ! command -v jq &> /dev/null; then
        echo -e "${YELLOW}Installing jq...${NC}"
        brew install jq
    else
        echo -e "${GREEN}✓ jq already installed${NC}"
    fi
}

# Function to install on Linux
install_linux() {
    # Install AWS CLI
    if ! command -v aws &> /dev/null; then
        echo -e "${YELLOW}Installing AWS CLI...${NC}"
        curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip"
        unzip awscliv2.zip
        sudo ./aws/install
        rm -rf awscliv2.zip aws/
    else
        echo -e "${GREEN}✓ AWS CLI already installed${NC}"
    fi
    
    # Install SAM CLI
    if ! command -v sam &> /dev/null; then
        echo -e "${YELLOW}Installing SAM CLI...${NC}"
        
        # Check for Python pip
        if ! command -v pip3 &> /dev/null; then
            echo -e "${YELLOW}Installing pip3...${NC}"
            sudo apt-get update
            sudo apt-get install -y python3-pip
        fi
        
        pip3 install aws-sam-cli --user
        
        # Add to PATH if needed
        if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
            echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
            export PATH="$HOME/.local/bin:$PATH"
        fi
    else
        echo -e "${GREEN}✓ SAM CLI already installed${NC}"
    fi
    
    # Install jq
    if ! command -v jq &> /dev/null; then
        echo -e "${YELLOW}Installing jq...${NC}"
        sudo apt-get update
        sudo apt-get install -y jq
    else
        echo -e "${GREEN}✓ jq already installed${NC}"
    fi
}

# Install based on OS
if [ "$OS" = "macos" ]; then
    install_macos
elif [ "$OS" = "linux" ]; then
    install_linux
fi

echo ""
echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}     Installation Complete!${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""

# Check versions
echo -e "${YELLOW}Installed versions:${NC}"
aws --version 2>/dev/null || echo "AWS CLI not found in PATH"
sam --version 2>/dev/null || echo "SAM CLI not found in PATH"
jq --version 2>/dev/null || echo "jq not found in PATH"

echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Configure AWS credentials:"
echo "   aws configure"
echo ""
echo "2. Enter your AWS credentials when prompted:"
echo "   - AWS Access Key ID"
echo "   - AWS Secret Access Key"
echo "   - Default region: eu-west-2"
echo "   - Default output format: json"
echo ""
echo "3. Run the migration:"
echo "   ./run-migration.sh"
echo ""

# Check if AWS is configured
if aws sts get-caller-identity &> /dev/null; then
    echo -e "${GREEN}✓ AWS credentials are already configured${NC}"
    ACCOUNT_ID=$(aws sts get-caller-identity --query 'Account' --output text)
    echo -e "  Account ID: ${YELLOW}${ACCOUNT_ID}${NC}"
else
    echo -e "${YELLOW}⚠️  AWS credentials not yet configured${NC}"
    echo "   Run: aws configure"
fi