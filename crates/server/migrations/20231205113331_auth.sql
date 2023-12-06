CREATE TABLE verification_token (
  identifier TEXT NOT NULL,
  expires TIMESTAMPTZ NOT NULL,
  token TEXT NOT NULL,
  PRIMARY KEY (identifier, token)
);

CREATE TABLE accounts (
  id SERIAL,
  "userId" INTEGER NOT NULL,
  type VARCHAR(255) NOT NULL,
  provider VARCHAR(255) NOT NULL,
  "providerAccountId" VARCHAR(255) NOT NULL,
  refresh_token TEXT,
  access_token TEXT,
  expires_at BIGINT,
  id_token TEXT,
  scope TEXT,
  session_state TEXT,
  token_type TEXT,
  PRIMARY KEY (id)
);

CREATE TABLE sessions (
  id SERIAL,
  "userId" INTEGER NOT NULL,
  expires TIMESTAMPTZ NOT NULL,
  "sessionToken" VARCHAR(255) NOT NULL,
  PRIMARY KEY (id)
);

CREATE TABLE users (
  id SERIAL,
  name VARCHAR(255),
  email VARCHAR(255),
  "emailVerified" TIMESTAMPTZ,
  image TEXT,
  PRIMARY KEY (id)
);

-- Rename 'team' table to 'projects'
ALTER TABLE
  "team" RENAME TO "projects";

-- Create 'users_projects' table
CREATE TABLE "users_projects" (
  "user_id" INTEGER NOT NULL,
  "project_id" INTEGER NOT NULL,
  PRIMARY KEY ("user_id", "project_id"),
  FOREIGN KEY ("user_id") REFERENCES "users" ("id"),
  FOREIGN KEY ("project_id") REFERENCES "projects" ("id")
);

-- Rename 'team_id' column to 'project_id' in 'simulations' table
ALTER TABLE
  "simulations" RENAME COLUMN "team_id" TO "project_id";