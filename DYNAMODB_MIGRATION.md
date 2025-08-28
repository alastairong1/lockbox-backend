# DynamoDB Table Migration Guide

This guide explains how to recreate the DynamoDB tables with the correct attribute names (camelCase) to match the model serialization.

## Why This Migration is Needed

The DynamoDB tables were originally created with snake_case attribute names (e.g., `invite_code`, `box_id`) but the Rust models serialize to camelCase (e.g., `inviteCode`, `boxId`) due to serde rename attributes. This mismatch causes queries to fail.

## Migration Overview

Since DynamoDB doesn't allow renaming attributes in existing Global Secondary Indexes, we need to:
1. Backup existing data
2. Delete and recreate tables with correct schema
3. Restore data with field name transformations

## Step-by-Step Migration Process

### Prerequisites

- AWS CLI configured with proper credentials
- `jq` installed for JSON processing (`brew install jq` on macOS)
- Access to the AWS account with the CloudFormation stack

### Step 1: Make Scripts Executable

```bash
chmod +x backup-dynamodb-tables.sh
chmod +x restore-dynamodb-tables.sh
```

### Step 2: Backup Current Data

```bash
# Set your AWS region (default is eu-west-2)
export AWS_REGION=eu-west-2

# Run the backup script
./backup-dynamodb-tables.sh
```

This will create a timestamped backup directory (e.g., `dynamodb-backup-20240828_143022/`) containing:
- `invitations-table.json` - All invitation records
- `box-table.json` - All box records

**⚠️ IMPORTANT**: Keep note of the backup directory name, you'll need it for restoration.

### Step 3: Deploy Migration Template

The migration template (`template-migration.yaml`) renames the logical resource IDs which forces CloudFormation to delete the old tables and create new ones.

```bash
# Deploy the migration template
sam deploy \
  --template-file template-migration.yaml \
  --stack-name lockbox-box-service \
  --capabilities CAPABILITY_IAM CAPABILITY_AUTO_EXPAND \
  --no-confirm-changeset \
  --s3-bucket lockbox-deployment-bucket-eu-west-2 \
  --region eu-west-2 \
  --no-fail-on-empty-changeset \
  --parameter-overrides "Stage=prod"
```

**What happens during this deployment:**
1. CloudFormation sees the resource names have changed (BoxesTable → BoxesTableV2)
2. It creates new tables with the correct camelCase attributes
3. It deletes the old tables (after creating the new ones)
4. The Lambda functions automatically point to the new tables

### Step 4: Wait for Stack Update to Complete

Monitor the CloudFormation stack in the AWS Console or use:

```bash
aws cloudformation wait stack-update-complete \
  --stack-name lockbox-box-service \
  --region eu-west-2
```

### Step 5: Restore Data

Once the new tables are created, restore the backed-up data:

```bash
# Use the backup directory name from Step 2
./restore-dynamodb-tables.sh dynamodb-backup-20240828_143022
```

The restore script will:
- Transform snake_case field names to camelCase automatically
- Put each item into the new tables
- Show progress as it restores

### Step 6: Verify Data

Check that data was restored correctly:

```bash
# Check invitations table
aws dynamodb scan \
  --table-name invitations-table \
  --region eu-west-2 \
  --query 'Count'

# Check box table  
aws dynamodb scan \
  --table-name box-table \
  --region eu-west-2 \
  --query 'Count'
```

### Step 7: Deploy Normal Template

Finally, deploy the regular template to restore the original resource names:

```bash
sam deploy \
  --template-file template.yaml \
  --stack-name lockbox-box-service \
  --capabilities CAPABILITY_IAM CAPABILITY_AUTO_EXPAND \
  --no-confirm-changeset \
  --s3-bucket lockbox-deployment-bucket-eu-west-2 \
  --region eu-west-2 \
  --no-fail-on-empty-changeset \
  --parameter-overrides "Stage=prod"
```

This won't affect the tables (they keep the same names) but restores the CloudFormation resource names to their original values.

## Rollback Plan

If something goes wrong:

1. **If data wasn't restored correctly**: 
   - The backup files are still in the backup directory
   - You can run the restore script again

2. **If you need to revert everything**:
   - Deploy the original template.yaml (without the camelCase fixes)
   - Restore from AWS Backup if you have it enabled
   - Or recreate tables manually and restore from your JSON backups

## Testing After Migration

Run the integration tests to verify everything works:

```bash
./run-all-tests.sh
```

## Notes

- The migration preserves all data including timestamps and IDs
- The restore script handles both old (snake_case) and new (camelCase) formats
- Point-in-time recovery is enabled on the new tables
- Backup plans continue to work with the new tables

## Troubleshooting

**Q: CloudFormation update fails with "Cannot update attribute definitions"**
A: Make sure you're using the migration template first, not trying to update the existing template directly.

**Q: Restore script says "No items to restore"**
A: Check that the backup files exist and contain data. Run `cat dynamodb-backup-*/invitations-table.json | jq '.Count'`

**Q: Application can't find data after migration**
A: Ensure the restore script completed successfully and check CloudWatch logs for any errors.