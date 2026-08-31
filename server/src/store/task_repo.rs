//! Task 仓储：tasks 表的读写。

use crate::store::Db;
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// `tasks` 表行。
#[derive(Debug, Clone, FromRow)]
pub struct TaskRow {
    pub id: Uuid,
    pub name: String,
    pub kind: String,
    pub params: Value,
    pub timeout_secs: Option<i32>,
}

/// tasks 表仓储。
#[derive(Clone)]
pub struct TaskRepo {
    db: Db,
}

impl TaskRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 新建任务。
    pub async fn create(&self, name: &str, kind: &str, params: Value) -> sqlx::Result<TaskRow> {
        sqlx::query_as::<_, TaskRow>(
            "INSERT INTO tasks (name, kind, params) VALUES ($1, $2, $3)
             RETURNING id, name, kind, params, timeout_secs",
        )
        .bind(name)
        .bind(kind)
        .bind(params)
        .fetch_one(self.db.pool())
        .await
    }
}
