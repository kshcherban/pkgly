UPDATE users SET email = username WHERE email IS NULL;
ALTER TABLE users ALTER COLUMN email SET NOT NULL;
