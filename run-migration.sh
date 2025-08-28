#!/bin/bash

# Automated DynamoDB Migration Script
# This script handles the complete migration process

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REGION="${AWS_REGION:-eu-west-2}"
STACK_NAME="lockbox-box-service"
S3_BUCKET="lockbox-deployment-bucket-${REGION}"

echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}     DynamoDB Table Migration Script${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""
echo -e "Region: ${YELLOW}${REGION}${NC}"
echo -e "Stack: ${YELLOW}${STACK_NAME}${NC}"
echo -e "S3 Bucket: ${YELLOW}${S3_BUCKET}${NC}"
echo ""

# Function to check prerequisites
check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"
    
    # Check AWS CLI
    if ! command -v aws &> /dev/null; then
        echo -e "${RED}Error: AWS CLI is not installed${NC}"
        echo "Please install AWS CLI: https://aws.amazon.com/cli/"
        exit 1
    fi
    
    # Check SAM CLI
    if ! command -v sam &> /dev/null; then
        echo -e "${RED}Error: SAM CLI is not installed${NC}"
        echo "Please install SAM CLI: pip install aws-sam-cli"
        exit 1
    fi
    
    # Check jq
    if ! command -v jq &> /dev/null; then
        echo -e "${RED}Error: jq is not installed${NC}"
        echo "Please install jq: brew install jq (macOS) or apt-get install jq (Linux)"
        exit 1
    fi
    
    # Check AWS credentials
    if ! aws sts get-caller-identity &> /dev/null; then
        echo -e "${RED}Error: AWS credentials not configured${NC}"
        echo "Please configure AWS CLI: aws configure"
        exit 1
    fi
    
    echo -e "${GREEN}✓ All prerequisites met${NC}"
    echo ""
}

# Function to wait for user confirmation
confirm_step() {
    local message=$1
    echo -e "${YELLOW}${message}${NC}"
    read -p "Press Enter to continue or Ctrl+C to abort... "
    echo ""
}

# Main migration process
main() {
    # Step 0: Check prerequisites
    check_prerequisites
    
    # Step 1: Backup current data
    echo -e "${GREEN}Step 1: Backing up current DynamoDB tables${NC}"
    confirm_step "This will backup all data from your DynamoDB tables. Continue?"
    
    ./backup-dynamodb-tables.sh
    
    # Get the backup directory name
    BACKUP_DIR=$(ls -dt dynamodb-backup-* | head -1)
    
    if [ -z "$BACKUP_DIR" ]; then
        echo -e "${RED}Error: No backup directory found${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}✓ Backup completed: ${BACKUP_DIR}${NC}"
    echo ""
    
    # Show backup summary
    if [ -f "${BACKUP_DIR}/invitations-table.json" ]; then
        INVITE_COUNT=$(jq '.Count' "${BACKUP_DIR}/invitations-table.json")
        echo -e "  Invitations backed up: ${YELLOW}${INVITE_COUNT}${NC}"
    fi
    
    if [ -f "${BACKUP_DIR}/box-table.json" ]; then
        BOX_COUNT=$(jq '.Count' "${BACKUP_DIR}/box-table.json")
        echo -e "  Boxes backed up: ${YELLOW}${BOX_COUNT}${NC}"
    fi
    echo ""
    
    # Step 2: Build Lambda functions
    echo -e "${GREEN}Step 2: Building Lambda functions${NC}"
    confirm_step "This will build the Lambda functions. Continue?"
    
    echo "Building Lambda functions..."
    cargo build --release --target x86_64-unknown-linux-musl
    
    # Package Lambda functions
    cp target/x86_64-unknown-linux-musl/release/lockbox-box-service bootstrap
    zip -q box-service.zip bootstrap
    
    cp target/x86_64-unknown-linux-musl/release/lockbox-invitation-service bootstrap
    zip -q invitation-service.zip bootstrap
    
    cp target/x86_64-unknown-linux-musl/release/invitation-event-service bootstrap
    zip -q invitation-event-handler.zip bootstrap
    
    rm bootstrap
    
    echo -e "${GREEN}✓ Lambda functions built${NC}"
    echo ""
    
    # Step 3: Deploy migration template
    echo -e "${GREEN}Step 3: Deploying migration template${NC}"
    echo -e "${YELLOW}⚠️  WARNING: This will DELETE and RECREATE your DynamoDB tables!${NC}"
    echo -e "${YELLOW}   Make sure you have backed up your data (Step 1 completed).${NC}"
    confirm_step "Deploy migration template to recreate tables?"
    
    sam deploy \
        --template-file template-migration.yaml \
        --stack-name "${STACK_NAME}" \
        --capabilities CAPABILITY_IAM CAPABILITY_AUTO_EXPAND \
        --no-confirm-changeset \
        --s3-bucket "${S3_BUCKET}" \
        --region "${REGION}" \
        --no-fail-on-empty-changeset \
        --parameter-overrides "Stage=prod"
    
    echo -e "${GREEN}✓ Migration template deployed${NC}"
    echo ""
    
    # Wait for stack to be ready
    echo "Waiting for CloudFormation stack to complete..."
    aws cloudformation wait stack-update-complete \
        --stack-name "${STACK_NAME}" \
        --region "${REGION}" 2>/dev/null || true
    
    # Step 4: Restore data
    echo -e "${GREEN}Step 4: Restoring data to new tables${NC}"
    confirm_step "This will restore the backed-up data to the new tables. Continue?"
    
    ./restore-dynamodb-tables.sh "${BACKUP_DIR}"
    
    echo -e "${GREEN}✓ Data restored${NC}"
    echo ""
    
    # Step 5: Deploy normal template
    echo -e "${GREEN}Step 5: Deploying normal template (final step)${NC}"
    confirm_step "This will finalize the migration by restoring the original resource names. Continue?"
    
    sam deploy \
        --template-file template.yaml \
        --stack-name "${STACK_NAME}" \
        --capabilities CAPABILITY_IAM CAPABILITY_AUTO_EXPAND \
        --no-confirm-changeset \
        --s3-bucket "${S3_BUCKET}" \
        --region "${REGION}" \
        --no-fail-on-empty-changeset \
        --parameter-overrides "Stage=prod"
    
    echo -e "${GREEN}✓ Normal template deployed${NC}"
    echo ""
    
    # Step 6: Verify
    echo -e "${GREEN}Step 6: Verifying migration${NC}"
    
    # Check item counts
    echo "Checking restored data..."
    
    CURRENT_INVITE_COUNT=$(aws dynamodb scan \
        --table-name invitations-table \
        --region "${REGION}" \
        --query 'Count' \
        --output text 2>/dev/null || echo "0")
    
    CURRENT_BOX_COUNT=$(aws dynamodb scan \
        --table-name box-table \
        --region "${REGION}" \
        --query 'Count' \
        --output text 2>/dev/null || echo "0")
    
    echo -e "  Current invitations: ${YELLOW}${CURRENT_INVITE_COUNT}${NC}"
    echo -e "  Current boxes: ${YELLOW}${CURRENT_BOX_COUNT}${NC}"
    
    # Final summary
    echo ""
    echo -e "${GREEN}==================================================${NC}"
    echo -e "${GREEN}     Migration Complete!${NC}"
    echo -e "${GREEN}==================================================${NC}"
    echo ""
    echo -e "Backup saved in: ${YELLOW}${BACKUP_DIR}${NC}"
    echo -e "Tables migrated with correct camelCase attributes"
    echo ""
    echo -e "${YELLOW}Next steps:${NC}"
    echo "1. Test your application to ensure everything works"
    echo "2. Run integration tests: ./run-all-tests.sh"
    echo "3. Keep the backup directory until you're sure everything is working"
    echo ""
}

# Run main function
main "$@"