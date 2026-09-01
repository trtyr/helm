-- hosts 加 addr 字段（forward 模式的拨号地址；reverse 模式为空）。
ALTER TABLE hosts ADD COLUMN addr TEXT NOT NULL DEFAULT '';
