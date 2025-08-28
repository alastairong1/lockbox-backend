#!/bin/bash

# Backup DynamoDB tables before recreation
# This script creates JSON backups of the DynamoDB tables

set -e

# Configuration
REGION="${AWS_REGION:-eu-west-2}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="dynamodb-backup-${TIMESTAMP}"

echo "Creating backup directory: ${BACKUP_DIR}"
mkdir -p "${BACKUP_DIR}"

# Function to backup a table
backup_table() {
    local TABLE_NAME=$1
    local OUTPUT_FILE="${BACKUP_DIR}/${TABLE_NAME}.json"
    
    echo "Checking if table ${TABLE_NAME} exists..."
    if aws dynamodb describe-table --table-name "${TABLE_NAME}" --region "${REGION}" >/dev/null 2>&1; then
        echo "Backing up table: ${TABLE_NAME}"
        
        # Scan the table and save to file
        aws dynamodb scan \
            --table-name "${TABLE_NAME}" \
            --region "${REGION}" \
            --output json \
            > "${OUTPUT_FILE}"
        
        # Get item count
        ITEM_COUNT=$(jq '.Count' "${OUTPUT_FILE}")
        echo "  Backed up ${ITEM_COUNT} items to ${OUTPUT_FILE}"
    else
        echo "  Table ${TABLE_NAME} does not exist, skipping..."
    fi
}

# Backup both tables
echo "Starting DynamoDB backup process..."
echo "Region: ${REGION}"
echo "----------------------------------------"

backup_table "invitations-table"
backup_table "box-table"

echo "----------------------------------------"
echo "Backup complete!"
echo "Backup location: ${BACKUP_DIR}"
echo ""
echo "To restore after recreation, use: ./restore-dynamodb-tables.sh ${BACKUP_DIR}"