CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username TEXT,
    email TEXT,
    name TEXT,
    password_hash TEXT,
    role TEXT DEFAULT 'user',
    wallet_balance DOUBLE PRECISION DEFAULT 0,
    created_at TIMESTAMP DEFAULT now()
);

CREATE TABLE IF NOT EXISTS products (
    id SERIAL PRIMARY KEY,
    sku TEXT,
    name TEXT,
    price DOUBLE PRECISION,
    stock INTEGER,
    created_at TIMESTAMP DEFAULT now()
);

CREATE TABLE IF NOT EXISTS coupons (
    id SERIAL PRIMARY KEY,
    code TEXT,
    percent INTEGER,
    expires TEXT,
    single_use BOOLEAN DEFAULT FALSE,
    used_count INTEGER DEFAULT 0
);

CREATE TABLE IF NOT EXISTS orders (
    id SERIAL PRIMARY KEY,
    order_no INTEGER,
    user_id INTEGER,
    status TEXT,
    total DOUBLE PRECISION,
    paid_amount DOUBLE PRECISION DEFAULT 0,
    refunded DOUBLE PRECISION DEFAULT 0,
    coupon TEXT,
    notify_url TEXT,
    created_at TIMESTAMP
);

CREATE TABLE IF NOT EXISTS order_items (
    id SERIAL PRIMARY KEY,
    order_id INTEGER,
    product_id INTEGER,
    quantity INTEGER,
    price DOUBLE PRECISION
);

CREATE TABLE IF NOT EXISTS payments (
    id SERIAL PRIMARY KEY,
    order_id INTEGER,
    amount DOUBLE PRECISION,
    method TEXT,
    bank_ref TEXT,
    status TEXT,
    created_at TIMESTAMP DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_orders_user_id ON orders(user_id);
CREATE INDEX IF NOT EXISTS idx_order_items_order_id ON order_items(order_id);
CREATE INDEX IF NOT EXISTS idx_payments_order_id ON payments(order_id);
