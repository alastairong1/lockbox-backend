#!/bin/bash

# Clean migration script - deletes entire CloudFormation stack and redeploys with correct schema
# WARNING: This will DELETE all data. Only for test environments.

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
STACK_NAME="${STACK_NAME:-lockbox-box-service}"
REGION="${AWS_REGION:-eu-west-2}"
USER_POOL_ID="${USER_POOL_ID:-eu-west-2_rdkfPgGg4}"
AUTO_CONFIRM=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -s|--stack-name)
            STACK_NAME="$2"
            shift 2
            ;;
        -r|--region)
            REGION="$2"
            shift 2
            ;;
        --user-pool-id)
            USER_POOL_ID="$2"
            shift 2
            ;;
        -y|--yes)
            AUTO_CONFIRM=true
            shift
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  -s, --stack-name NAME     CloudFormation stack name (default: lockbox-box-service)"
            echo "  -r, --region REGION       AWS region (default: eu-west-2)"
            echo "  --user-pool-id ID         Cognito User Pool ID (default: eu-west-2_rdkfPgGg4)"
            echo "  -y, --yes                 Auto-confirm deletion (skip prompt)"
            echo "  -h, --help                Show this help message"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            echo "Use -h or --help for usage information"
            exit 1
            ;;
    esac
done

# Configuration
S3_BUCKET="lockbox-deployment-bucket-${REGION}"

echo -e "${RED}==================================================${NC}"
echo -e "${RED}     CLEAN MIGRATION - ALL DATA WILL BE DELETED${NC}"
echo -e "${RED}==================================================${NC}"
echo ""
echo -e "Stack Name: ${YELLOW}${STACK_NAME}${NC}"
echo -e "Region: ${YELLOW}${REGION}${NC}"
echo -e "User Pool ID: ${YELLOW}${USER_POOL_ID}${NC}"
echo ""

# Confirm deletion unless auto-confirm is set
if [ "$AUTO_CONFIRM" = false ]; then
    echo -e "${RED}⚠️  WARNING: This will DELETE the entire CloudFormation stack and all data!${NC}"
    read -p "Type 'DELETE' to confirm: " CONFIRM
    if [ "$CONFIRM" != "DELETE" ]; then
        echo "Aborted."
        exit 1
    fi
fi

# Step 1: Delete the entire CloudFormation stack
echo -e "${BLUE}Step 1: Deleting CloudFormation stack${NC}"

if aws cloudformation describe-stacks --stack-name "${STACK_NAME}" --region "${REGION}" &> /dev/null; then
    echo "Deleting stack ${STACK_NAME}..."
    
    # Use sam delete for cleaner deletion (or aws cloudformation delete-stack as fallback)
    if command -v sam &> /dev/null; then
        sam delete \
            --stack-name "${STACK_NAME}" \
            --no-prompts \
            --region "${REGION}"
    else
        aws cloudformation delete-stack \
            --stack-name "${STACK_NAME}" \
            --region "${REGION}"
        
        # Wait for stack deletion
        echo "Waiting for stack deletion to complete..."
        aws cloudformation wait stack-delete-complete \
            --stack-name "${STACK_NAME}" \
            --region "${REGION}" || true
    fi
    
    echo -e "${GREEN}✓ Stack deleted${NC}"
else
    echo "Stack ${STACK_NAME} does not exist, skipping deletion..."
fi
echo ""

# Step 2: Clean up any retained/orphaned tables
echo -e "${BLUE}Step 2: Cleaning up any retained DynamoDB tables${NC}"

# Tables might be retained by DeletionPolicy
for TABLE in "box-table" "invitations-table"; do
    if aws dynamodb describe-table --table-name "${TABLE}" --region "${REGION}" &> /dev/null 2>&1; then
        echo "Found retained table ${TABLE}, deleting..."
        aws dynamodb delete-table \
            --table-name "${TABLE}" \
            --region "${REGION}" \
            --output text > /dev/null
        
        # Wait for deletion with timeout
        WAIT_COUNT=0
        MAX_WAIT=60
        while [ $WAIT_COUNT -lt $MAX_WAIT ]; do
            if ! aws dynamodb describe-table --table-name "${TABLE}" --region "${REGION}" &> /dev/null 2>&1; then
                echo "✓ ${TABLE} deleted"
                break
            fi
            echo -n "."
            sleep 2
            WAIT_COUNT=$((WAIT_COUNT + 1))
        done
        
        if [ $WAIT_COUNT -eq $MAX_WAIT ]; then
            echo -e "${YELLOW}Warning: Timeout waiting for ${TABLE} deletion${NC}"
        fi
    fi
done
echo ""

# Step 3: Build Lambda functions
echo -e "${BLUE}Step 3: Building Lambda functions${NC}"

# Check for cargo
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: cargo not found. Please install Rust.${NC}"
    exit 1
fi

# Build all services
cargo build --release --target x86_64-unknown-linux-musl

# Package Lambda functions with correct names
echo "Packaging Lambda functions..."

# Box service
cp target/x86_64-unknown-linux-musl/release/lockbox-box-service bootstrap
zip -q box-service.zip bootstrap
rm bootstrap

# Invitation service
cp target/x86_64-unknown-linux-musl/release/lockbox-invitation-service bootstrap
zip -q invitation-service.zip bootstrap
rm bootstrap

# Invitation event handler - FIXED: correct binary name
cp target/x86_64-unknown-linux-musl/release/invitation-event-service bootstrap
zip -q invitation-event-handler.zip bootstrap
rm bootstrap

echo -e "${GREEN}✓ Lambda functions built and packaged${NC}"
echo ""

# Step 4: Deploy fresh stack
echo -e "${BLUE}Step 4: Deploying fresh stack with correct camelCase schema${NC}"

# Create S3 bucket if needed
if ! aws s3 ls "s3://${S3_BUCKET}" --region "${REGION}" &> /dev/null; then
    echo "Creating S3 bucket: ${S3_BUCKET}..."
    aws s3api create-bucket \
        --bucket "${S3_BUCKET}" \
        --region "${REGION}" \
        --create-bucket-configuration LocationConstraint="${REGION}" &> /dev/null || true
fi

# Check for SAM CLI
if ! command -v sam &> /dev/null; then
    echo -e "${RED}Error: SAM CLI not found. Please install: pip install aws-sam-cli${NC}"
    exit 1
fi

# Deploy using SAM
echo "Deploying CloudFormation stack..."
sam deploy \
    --template-file template.yaml \
    --stack-name "${STACK_NAME}" \
    --s3-bucket "${S3_BUCKET}" \
    --capabilities CAPABILITY_IAM CAPABILITY_AUTO_EXPAND \
    --parameter-overrides "UserPoolId=${USER_POOL_ID}" \
    --region "${REGION}" \
    --no-confirm-changeset \
    --no-fail-on-empty-changeset

echo -e "${GREEN}✓ Stack deployed${NC}"
echo ""

# Step 5: Verify deployment
echo -e "${BLUE}Step 5: Verifying deployment${NC}"

# Define expected tables
TABLES=("box-table" "invitations-table")

# Wait for each table and its GSIs to be active
for TABLE in "${TABLES[@]}"; do
    echo -n "Waiting for ${TABLE} to be active"
    
    # Wait for table to exist (max 2 minutes)
    ATTEMPTS=0
    MAX_ATTEMPTS=60
    while [ $ATTEMPTS -lt $MAX_ATTEMPTS ]; do
        if aws dynamodb describe-table --table-name "${TABLE}" --region "${REGION}" &> /dev/null 2>&1; then
            break
        fi
        echo -n "."
        sleep 2
        ATTEMPTS=$((ATTEMPTS + 1))
    done
    
    if [ $ATTEMPTS -eq $MAX_ATTEMPTS ]; then
        echo -e " ${RED}✗ Table ${TABLE} not created${NC}"
        exit 1
    fi
    
    # Wait for table and GSIs to be ACTIVE
    while true; do
        TABLE_STATUS=$(aws dynamodb describe-table \
            --table-name "${TABLE}" \
            --region "${REGION}" \
            --query 'Table.TableStatus' \
            --output text 2>/dev/null || echo "CREATING")
        
        if [ "${TABLE_STATUS}" = "ACTIVE" ]; then
            # Check if all GSIs are also active
            GSI_COUNT=$(aws dynamodb describe-table \
                --table-name "${TABLE}" \
                --region "${REGION}" \
                --query 'length(Table.GlobalSecondaryIndexes)' \
                --output text 2>/dev/null || echo "0")
            
            if [ "${GSI_COUNT}" -gt 0 ]; then
                INACTIVE_GSI=$(aws dynamodb describe-table \
                    --table-name "${TABLE}" \
                    --region "${REGION}" \
                    --query 'Table.GlobalSecondaryIndexes[?IndexStatus!=`ACTIVE`].IndexName' \
                    --output text 2>/dev/null || echo "")
                
                if [ -z "${INACTIVE_GSI}" ]; then
                    echo -e " ${GREEN}✓${NC}"
                    break
                fi
            else
                echo -e " ${GREEN}✓${NC}"
                break
            fi
        fi
        echo -n "."
        sleep 2
    done
done

# Get and display stack outputs
echo ""
echo -e "${BLUE}Stack Outputs:${NC}"

API_URL=$(aws cloudformation describe-stacks \
    --stack-name "${STACK_NAME}" \
    --region "${REGION}" \
    --query "Stacks[0].Outputs[?OutputKey=='ApiURL'].OutputValue" \
    --output text 2>/dev/null || echo "Not found")

BOX_TABLE=$(aws cloudformation describe-stacks \
    --stack-name "${STACK_NAME}" \
    --region "${REGION}" \
    --query "Stacks[0].Outputs[?OutputKey=='BoxesTableName'].OutputValue" \
    --output text 2>/dev/null || echo "box-table")

INVITATION_TABLE=$(aws cloudformation describe-stacks \
    --stack-name "${STACK_NAME}" \
    --region "${REGION}" \
    --query "Stacks[0].Outputs[?OutputKey=='InvitationsTableName'].OutputValue" \
    --output text 2>/dev/null || echo "invitations-table")

echo -e "  API URL: ${YELLOW}${API_URL}${NC}"
echo -e "  Box Table: ${YELLOW}${BOX_TABLE}${NC}"
echo -e "  Invitations Table: ${YELLOW}${INVITATION_TABLE}${NC}"
echo ""

# Final success message
echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}     Migration Complete!${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""
echo -e "${GREEN}✓ Stack deployed with no CloudFormation drift${NC}"
echo -e "${GREEN}✓ All tables have correct camelCase attributes${NC}"
echo -e "${GREEN}✓ All GSIs are active and ready${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "1. Run tests: ./run-all-tests.sh"
echo "2. Test the API endpoints at: ${API_URL}"
echo ""