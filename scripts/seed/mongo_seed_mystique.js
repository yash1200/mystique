/**
 * Seed MongoDB database `mystique` with ~10k related documents.
 * Run via mongosh as admin.
 *
 * Collections:
 *   categories → products → users → orders → order_items
 *   (plus reviews linking users + products)
 */
const DB_NAME = "mystique";
const db = db.getSiblingDB(DB_NAME);

print(`Dropping existing collections in ${DB_NAME}…`);
db.categories.drop();
db.products.drop();
db.users.drop();
db.orders.drop();
db.order_items.drop();
db.reviews.drop();

const now = new Date();
const dayMs = 86400000;

function pick(arr) {
  return arr[Math.floor(Math.random() * arr.length)];
}

function randInt(min, max) {
  return min + Math.floor(Math.random() * (max - min + 1));
}

// ── categories (20) ──────────────────────────────────────────
const categoryNames = [
  "Electronics", "Books", "Clothing", "Home", "Sports",
  "Toys", "Beauty", "Grocery", "Automotive", "Garden",
  "Music", "Office", "Pets", "Health", "Travel",
  "Games", "Baby", "Jewelry", "Tools", "Outdoors",
];
const categories = categoryNames.map((name, i) => ({
  _id: ObjectId(),
  slug: name.toLowerCase(),
  name,
  sortOrder: i,
  createdAt: new Date(now - i * dayMs),
}));
db.categories.insertMany(categories);
print(`categories: ${categories.length}`);

// ── users (2000) ─────────────────────────────────────────────
const first = ["Ada", "Lin", "Sam", "Jordan", "Riley", "Casey", "Morgan", "Avery", "Quinn", "Jamie", "Alex", "Taylor", "Drew", "Blake", "Cameron", "Reese", "Skyler", "Parker", "Hayden", "Rowan"];
const last = ["Nguyen", "Patel", "Garcia", "Kim", "Smith", "Lee", "Brown", "Davis", "Wilson", "Moore", "Clark", "Lewis", "Young", "Hall", "Allen", "Wright", "Scott", "Green", "Baker", "Adams"];
const cities = ["Austin", "Seattle", "Berlin", "Tokyo", "Lagos", "Toronto", "Sydney", "Mumbai", "Lisbon", "Nairobi"];

const users = [];
for (let i = 0; i < 2000; i++) {
  const f = pick(first);
  const l = pick(last);
  users.push({
    _id: ObjectId(),
    email: `user${i}@example.com`,
    name: `${f} ${l}`,
    city: pick(cities),
    isActive: Math.random() > 0.08,
    createdAt: new Date(now - randInt(1, 900) * dayMs),
  });
}
db.users.insertMany(users, { ordered: false });
db.users.createIndex({ email: 1 }, { unique: true });
db.users.createIndex({ city: 1 });
print(`users: ${users.length}`);

// ── products (1500) ──────────────────────────────────────────
const adjectives = ["Soft", "Bright", "Classic", "Modern", "Compact", "Premium", "Eco", "Smart", "Ultra", "Lite"];
const nouns = ["Lamp", "Chair", "Bottle", "Speaker", "Bag", "Watch", "Keyboard", "Mug", "Notebook", "Headset", "Backpack", "Charger", "Camera", "Sneaker", "Jacket"];
const products = [];
for (let i = 0; i < 1500; i++) {
  const cat = pick(categories);
  products.push({
    _id: ObjectId(),
    sku: `SKU-${String(i).padStart(5, "0")}`,
    name: `${pick(adjectives)} ${pick(nouns)} ${i}`,
    categoryId: cat._id,
    categorySlug: cat.slug,
    priceCents: randInt(499, 99900),
    stock: randInt(0, 500),
    active: Math.random() > 0.05,
    createdAt: new Date(now - randInt(1, 600) * dayMs),
  });
}
db.products.insertMany(products, { ordered: false });
db.products.createIndex({ sku: 1 }, { unique: true });
db.products.createIndex({ categoryId: 1 });
db.products.createIndex({ categorySlug: 1 });
print(`products: ${products.length}`);

// ── orders (3500) ────────────────────────────────────────────
const statuses = ["pending", "paid", "shipped", "delivered", "cancelled"];
const orders = [];
for (let i = 0; i < 3500; i++) {
  const user = pick(users);
  orders.push({
    _id: ObjectId(),
    orderNumber: `ORD-${String(i).padStart(6, "0")}`,
    userId: user._id,
    userEmail: user.email,
    status: pick(statuses),
    totalCents: 0, // filled after items
    placedAt: new Date(now - randInt(0, 400) * dayMs),
  });
}
// insert later after totals — first create items then update totals in batches

// ── order_items (2200) + attach totals ───────────────────────
// total target ~10k: 20 + 2000 + 1500 + 3500 + 2200 + 780 = 10000
const orderItems = [];
for (let i = 0; i < 2200; i++) {
  const order = orders[i % orders.length];
  const product = pick(products);
  const qty = randInt(1, 5);
  const line = product.priceCents * qty;
  orderItems.push({
    _id: ObjectId(),
    orderId: order._id,
    orderNumber: order.orderNumber,
    productId: product._id,
    productSku: product.sku,
    productName: product.name,
    quantity: qty,
    unitPriceCents: product.priceCents,
    lineTotalCents: line,
  });
  order.totalCents += line;
}
// remaining orders without items get a synthetic total
for (const o of orders) {
  if (o.totalCents === 0) o.totalCents = randInt(999, 25000);
}

db.orders.insertMany(orders, { ordered: false });
db.orders.createIndex({ orderNumber: 1 }, { unique: true });
db.orders.createIndex({ userId: 1 });
db.orders.createIndex({ userEmail: 1 });
db.orders.createIndex({ status: 1, placedAt: -1 });
print(`orders: ${orders.length}`);

db.order_items.insertMany(orderItems, { ordered: false });
db.order_items.createIndex({ orderId: 1 });
db.order_items.createIndex({ productId: 1 });
print(`order_items: ${orderItems.length}`);

// ── reviews (780) ────────────────────────────────────────────
const reviews = [];
for (let i = 0; i < 780; i++) {
  const user = pick(users);
  const product = pick(products);
  reviews.push({
    _id: ObjectId(),
    userId: user._id,
    userEmail: user.email,
    productId: product._id,
    productSku: product.sku,
    rating: randInt(1, 5),
    title: `Review ${i}`,
    body: `Dummy review text #${i} for ${product.name}.`,
    createdAt: new Date(now - randInt(0, 300) * dayMs),
  });
}
db.reviews.insertMany(reviews, { ordered: false });
db.reviews.createIndex({ productId: 1 });
db.reviews.createIndex({ userId: 1 });
print(`reviews: ${reviews.length}`);

const counts = {
  categories: db.categories.countDocuments(),
  users: db.users.countDocuments(),
  products: db.products.countDocuments(),
  orders: db.orders.countDocuments(),
  order_items: db.order_items.countDocuments(),
  reviews: db.reviews.countDocuments(),
};
const total = Object.values(counts).reduce((a, b) => a + b, 0);
printjson(counts);
print(`TOTAL documents: ${total}`);
print("Done. URI: mongodb://admin:123@127.0.0.1:27017/mystique?authSource=admin");
