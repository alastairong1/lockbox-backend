#!/bin/bash

# Restore DynamoDB tables from backup
# This script restores data from JSON backups with field name transformations

set -e

# Check arguments
if [ $# -ne 1 ]; then
    echo "Usage: $0 <backup-directory>"
    exit 1
fi

BACKUP_DIR=$1
REGION="${AWS_REGION:-eu-west-2}"

if [ ! -d "${BACKUP_DIR}" ]; then
    echo "Error: Backup directory '${BACKUP_DIR}' does not exist"
    exit 1
fi

echo "Restoring from backup: ${BACKUP_DIR}"
echo "Region: ${REGION}"
echo "----------------------------------------"

# Function to transform and restore invitations table
restore_invitations() {
    local INPUT_FILE="${BACKUP_DIR}/invitations-table.json"
    local TABLE_NAME="invitations-table"
    
    if [ ! -f "${INPUT_FILE}" ]; then
        echo "No backup found for invitations-table, skipping..."
        return
    fi
    
    echo "Restoring invitations-table..."
    
    # Extract items and transform field names from snake_case to camelCase if needed
    ITEMS=$(jq -c '.Items[]' "${INPUT_FILE}")
    
    if [ -z "${ITEMS}" ]; then
        echo "  No items to restore"
        return
    fi
    
    # Count total items
    TOTAL=$(jq '.Count' "${INPUT_FILE}")
    COUNT=0
    
    # Process each item
    while IFS= read -r item; do
        # Transform field names if they're in snake_case
        # This handles both old (snake_case) and new (camelCase) formats
        TRANSFORMED_ITEM=$(echo "$item" | jq '
            if has("invite_code") then .inviteCode = .invite_code | del(.invite_code) else . end |
            if has("box_id") then .boxId = .box_id | del(.box_id) else . end |
            if has("invited_name") then .invitedName = .invited_name | del(.invited_name) else . end |
            if has("created_at") then .createdAt = .created_at | del(.created_at) else . end |
            if has("expires_at") then .expiresAt = .expires_at | del(.expires_at) else . end |
            if has("linked_user_id") then .linkedUserId = .linked_user_id | del(.linked_user_id) else . end |
            if has("creator_id") then .creatorId = .creator_id | del(.creator_id) else . end
        ')
        
        # Put item into table
        aws dynamodb put-item \
            --table-name "${TABLE_NAME}" \
            --item "${TRANSFORMED_ITEM}" \
            --region "${REGION}" \
            >/dev/null 2>&1
        
        COUNT=$((COUNT + 1))
        echo -ne "  Restored ${COUNT}/${TOTAL} items\r"
    done <<< "${ITEMS}"
    
    echo "  Restored ${COUNT} items to ${TABLE_NAME}    "
}

# Function to restore box table (no transformation needed)
restore_boxes() {
    local INPUT_FILE="${BACKUP_DIR}/box-table.json"
    local TABLE_NAME="box-table"
    
    if [ ! -f "${INPUT_FILE}" ]; then
        echo "No backup found for box-table, skipping..."
        return
    fi
    
    echo "Restoring box-table..."
    
    # Extract items
    ITEMS=$(jq -c '.Items[]' "${INPUT_FILE}")
    
    if [ -z "${ITEMS}" ]; then
        echo "  No items to restore"
        return
    fi
    
    # Count total items
    TOTAL=$(jq '.Count' "${INPUT_FILE}")
    COUNT=0
    
    # Process each item
    while IFS= read -r item; do
        # Put item into table (no transformation needed for box-table)
        aws dynamodb put-item \
            --table-name "${TABLE_NAME}" \
            --item "${item}" \
            --region "${REGION}" \
            >/dev/null 2>&1
        
        COUNT=$((COUNT + 1))
        echo -ne "  Restored ${COUNT}/${TOTAL} items\r"
    done <<< "${ITEMS}"
    
    echo "  Restored ${COUNT} items to ${TABLE_NAME}    "
}

# Wait for tables to be ready
wait_for_table() {
    local TABLE_NAME=$1
    echo "Waiting for ${TABLE_NAME} to be ready..."
    
    while true; do
        STATUS=$(aws dynamodb describe-table \
            --table-name "${TABLE_NAME}" \
            --region "${REGION}" \
            --query 'Table.TableStatus' \
            --output text 2>/dev/null || echo "NOTFOUND")
        
        if [ "${STATUS}" = "ACTIVE" ]; then
            echo "  ${TABLE_NAME} is ready"
            break
        elif [ "${STATUS}" = "NOTFOUND" ]; then
            echo "  Waiting for ${TABLE_NAME} to be created..."
        else
            echo "  ${TABLE_NAME} status: ${STATUS}"
        fi
        sleep 5
    done
}

# Wait for tables to exist and be active
wait_for_table "invitations-table"
wait_for_table "box-table"

# Restore the data
restore_invitations
restore_boxes

echo "----------------------------------------"
echo "Restore complete!"