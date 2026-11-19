-- v3: chỉ mục truy vấn thống kê theo thời gian; các cột mới trên `rotations` và `terminals`
-- được thêm idempotent qua `add_column_if_missing` trong `Store::initialize`.
CREATE INDEX IF NOT EXISTS idx_usage_ts_ms ON usage(ts_ms);
