#!/bin/bash

# Script to delete backup vault and all recovery points
# This is needed when CloudFormation stack deletion fails due to BackupVault

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

REGION="${AWS_REGION:-eu-west-2}"
VAULT_NAME="lockbox-box-service-backup-vault"

echo -e "${YELLOW}==================================================${NC}"
echo -e "${YELLOW}     Fixing Backup Vault Deletion Issue${NC}"
echo -e "${YELLOW}==================================================${NC}"
echo ""

# Step 1: Delete all recovery points
echo -e "${GREEN}Step 1: Deleting all recovery points from backup vault${NC}"

# Get all recovery points
RECOVERY_POINTS=$(aws backup list-recovery-points-by-backup-vault \
    --backup-vault-name "${VAULT_NAME}" \
    --region "${REGION}" \
    --query 'RecoveryPoints[].RecoveryPointArn' \
    --output text 2>/dev/null || echo "")

if [ -n "${RECOVERY_POINTS}" ]; then
    COUNT=$(echo "${RECOVERY_POINTS}" | wc -w)
    echo "Found ${COUNT} recovery points to delete..."
    
    DELETED=0
    for RECOVERY_POINT in ${RECOVERY_POINTS}; do
        echo -ne "Deleting recovery points: ${DELETED}/${COUNT}\r"
        aws backup delete-recovery-point \
            --backup-vault-name "${VAULT_NAME}" \
            --recovery-point-arn "${RECOVERY_POINT}" \
            --region "${REGION}" 2>/dev/null || true
        DELETED=$((DELETED + 1))
    done
    echo -e "Deleted ${DELETED} recovery points                    "
else
    echo "No recovery points found in backup vault"
fi

# Step 2: Delete the backup vault
echo -e "${GREEN}Step 2: Deleting backup vault${NC}"

if aws backup describe-backup-vault --backup-vault-name "${VAULT_NAME}" --region "${REGION}" &> /dev/null; then
    echo "Deleting backup vault: ${VAULT_NAME}"
    aws backup delete-backup-vault \
        --backup-vault-name "${VAULT_NAME}" \
        --region "${REGION}" 2>/dev/null || true
    echo -e "${GREEN}✓ Backup vault deleted${NC}"
else
    echo "Backup vault does not exist or already deleted"
fi

# Step 3: Now retry stack deletion
echo ""
echo -e "${GREEN}Step 3: Retrying stack deletion${NC}"

STACK_NAME="lockbox-box-service"

if aws cloudformation describe-stacks --stack-name "${STACK_NAME}" --region "${REGION}" &> /dev/null; then
    echo "Deleting CloudFormation stack: ${STACK_NAME}"
    
    aws cloudformation delete-stack \
        --stack-name "${STACK_NAME}" \
        --region "${REGION}"
    
    echo "Waiting for stack deletion to complete..."
    aws cloudformation wait stack-delete-complete \
        --stack-name "${STACK_NAME}" \
        --region "${REGION}" || true
    
    echo -e "${GREEN}✓ Stack deleted successfully${NC}"
else
    echo "Stack does not exist or already deleted"
fi

echo ""
echo -e "${GREEN}==================================================${NC}"
echo -e "${GREEN}     Backup Vault Issue Fixed!${NC}"
echo -e "${GREEN}==================================================${NC}"
echo ""
echo "You can now run the migration script:"
echo "  ./migrate.sh"
echo ""