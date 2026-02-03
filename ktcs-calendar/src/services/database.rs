//! Database persistence layer for stamp records
//!
//! This module provides SQLite-based persistence for stamp records,
//! enabling the calendar server to survive restarts without losing data.
//!
//! # Usage
//!
//! The Database is integrated into AppState and used by handlers to persist
//! stamp records across server restarts. See DbStampRecord for the database
//! representation of stamp records.

use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use tracing::info;

/// Database error type
#[derive(Debug, thiserror::Error)]
#[allow(dead_code)] // Variants for future error handling
pub enum DatabaseError {
    #[error("SQLx error: {0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("Failed to serialize proof: {0}")]
    SerializeError(String),

    #[error("Failed to deserialize proof: {0}")]
    DeserializeError(String),

    #[error("Record not found: {0}")]
    NotFound(String),
}

pub type Result<T> = std::result::Result<T, DatabaseError>;

/// Stamp status stored in database
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbStampStatus {
    Pending,
    Batched,
    Confirmed,
}

impl DbStampStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            DbStampStatus::Pending => "pending",
            DbStampStatus::Batched => "batched",
            DbStampStatus::Confirmed => "confirmed",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(DbStampStatus::Pending),
            "batched" => Some(DbStampStatus::Batched),
            "confirmed" => Some(DbStampStatus::Confirmed),
            _ => None,
        }
    }
}

/// Database record for a stamp
#[derive(Debug, Clone)]
pub struct DbStampRecord {
    pub id: String,
    pub digest: [u8; 32],
    pub status: DbStampStatus,
    pub submitted_at: i64,
    pub confirmed_at: Option<i64>,
    pub proof: Option<Vec<u8>>,
    pub batch_mode: String,
}

/// Database service for stamp persistence
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Create a new database connection
    ///
    /// If the database file doesn't exist, it will be created.
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Connecting to database: {}", database_url);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        let db = Self { pool };
        db.run_migrations().await?;

        info!("Database connection established");
        Ok(db)
    }

    /// Create an in-memory database (for testing)
    pub async fn in_memory() -> Result<Self> {
        Self::new("sqlite::memory:").await
    }

    /// Run database migrations
    async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations...");

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS stamps (
                id TEXT PRIMARY KEY,
                digest BLOB NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                submitted_at INTEGER NOT NULL,
                confirmed_at INTEGER,
                proof BLOB,
                batch_mode TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index on status for efficient queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_stamps_status ON stamps(status)
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Create index on submitted_at for time-based queries
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_stamps_submitted_at ON stamps(submitted_at)
            "#,
        )
        .execute(&self.pool)
        .await?;

        info!("Database migrations complete");
        Ok(())
    }

    /// Save a new stamp record
    pub async fn save_stamp(&self, record: &DbStampRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO stamps (id, digest, status, submitted_at, confirmed_at, proof, batch_mode)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                status = excluded.status,
                confirmed_at = excluded.confirmed_at,
                proof = excluded.proof,
                updated_at = strftime('%s', 'now')
            "#,
        )
        .bind(&record.id)
        .bind(&record.digest[..])
        .bind(record.status.as_str())
        .bind(record.submitted_at)
        .bind(record.confirmed_at)
        .bind(&record.proof)
        .bind(&record.batch_mode)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Get a stamp record by ID
    pub async fn get_stamp(&self, id: &str) -> Result<Option<DbStampRecord>> {
        let row = sqlx::query(
            r#"
            SELECT id, digest, status, submitted_at, confirmed_at, proof, batch_mode
            FROM stamps
            WHERE id = ?
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(row) => {
                let digest_bytes: Vec<u8> = row.get("digest");
                let mut digest = [0u8; 32];
                if digest_bytes.len() == 32 {
                    digest.copy_from_slice(&digest_bytes);
                }

                let status_str: String = row.get("status");
                let status = DbStampStatus::from_str(&status_str)
                    .unwrap_or(DbStampStatus::Pending);

                Ok(Some(DbStampRecord {
                    id: row.get("id"),
                    digest,
                    status,
                    submitted_at: row.get("submitted_at"),
                    confirmed_at: row.get("confirmed_at"),
                    proof: row.get("proof"),
                    batch_mode: row.get("batch_mode"),
                }))
            }
            None => Ok(None),
        }
    }

    /// Update stamp status
    pub async fn update_stamp_status(
        &self,
        id: &str,
        status: DbStampStatus,
        confirmed_at: Option<i64>,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE stamps
            SET status = ?, confirmed_at = ?, updated_at = strftime('%s', 'now')
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(confirmed_at)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Update stamp proof
    pub async fn update_stamp_proof(&self, id: &str, proof: &[u8]) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE stamps
            SET proof = ?, updated_at = strftime('%s', 'now')
            WHERE id = ?
            "#,
        )
        .bind(proof)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get all stamps with a given status
    /// Used for admin/maintenance operations
    #[allow(dead_code)]
    pub async fn get_stamps_by_status(&self, status: DbStampStatus) -> Result<Vec<DbStampRecord>> {
        let rows = sqlx::query(
            r#"
            SELECT id, digest, status, submitted_at, confirmed_at, proof, batch_mode
            FROM stamps
            WHERE status = ?
            ORDER BY submitted_at ASC
            "#,
        )
        .bind(status.as_str())
        .fetch_all(&self.pool)
        .await?;

        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            let digest_bytes: Vec<u8> = row.get("digest");
            let mut digest = [0u8; 32];
            if digest_bytes.len() == 32 {
                digest.copy_from_slice(&digest_bytes);
            }

            let status_str: String = row.get("status");
            let status = DbStampStatus::from_str(&status_str)
                .unwrap_or(DbStampStatus::Pending);

            records.push(DbStampRecord {
                id: row.get("id"),
                digest,
                status,
                submitted_at: row.get("submitted_at"),
                confirmed_at: row.get("confirmed_at"),
                proof: row.get("proof"),
                batch_mode: row.get("batch_mode"),
            });
        }

        Ok(records)
    }

    /// Count stamps by status
    pub async fn count_by_status(&self, status: DbStampStatus) -> Result<i64> {
        let row = sqlx::query(
            r#"
            SELECT COUNT(*) as count FROM stamps WHERE status = ?
            "#,
        )
        .bind(status.as_str())
        .fetch_one(&self.pool)
        .await?;

        Ok(row.get("count"))
    }

    /// Delete old confirmed stamps (for cleanup)
    /// Used for periodic maintenance
    pub async fn delete_old_stamps(&self, older_than_secs: i64) -> Result<u64> {
        let cutoff = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
            - older_than_secs;

        let result = sqlx::query(
            r#"
            DELETE FROM stamps
            WHERE status = 'confirmed' AND confirmed_at < ?
            "#,
        )
        .bind(cutoff)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_database_operations() {
        let db = Database::in_memory().await.unwrap();

        // Create a test record
        let record = DbStampRecord {
            id: "test_id_123".to_string(),
            digest: [1u8; 32],
            status: DbStampStatus::Pending,
            submitted_at: 1234567890,
            confirmed_at: None,
            proof: None,
            batch_mode: "instant".to_string(),
        };

        // Save
        db.save_stamp(&record).await.unwrap();

        // Get
        let retrieved = db.get_stamp("test_id_123").await.unwrap().unwrap();
        assert_eq!(retrieved.id, "test_id_123");
        assert_eq!(retrieved.status, DbStampStatus::Pending);

        // Update status
        db.update_stamp_status("test_id_123", DbStampStatus::Confirmed, Some(1234567899))
            .await
            .unwrap();

        let updated = db.get_stamp("test_id_123").await.unwrap().unwrap();
        assert_eq!(updated.status, DbStampStatus::Confirmed);
        assert_eq!(updated.confirmed_at, Some(1234567899));

        // Count
        let count = db.count_by_status(DbStampStatus::Confirmed).await.unwrap();
        assert_eq!(count, 1);
    }
}
