# Running server binary

`cargo run --bin server`

# DB

Read /migrations/README.md


# For Release
If you make changes to the db, run the following to ensure that docker builds succeed

```
cd crates/server
cargo sqlx prepare
```

## Manual Deployment
1. Configure AWS Cli: https://docs.aws.amazon.com/cli/latest/userguide/cli-chap-getting-started.html
2. Retrieve AWS credentials for Docker cli
```
aws ecr get-login-password --region us-east-1 | docker login --username AWS --password-stdin 414942293597.dkr.ecr.us-east-1.amazonaws.com
```
3. Build Walnut server docker image
```
docker build -t walnut-server-pipeline .
```
4. Tag Image
```
docker tag walnut-server-pipeline:latest 414942293597.dkr.ecr.us-east-1.amazonaws.com/walnut-server-pipeline:latest
```
5. Push Image
```
docker push 414942293597.dkr.ecr.us-east-1.amazonaws.com/walnut-server-pipeline:latest
```
6. Deploy the latest binaries
- Go to ECS -> Task Definitions
- Select `WalnutServer-east-1`
- Click `Create new revision`
- Keep the default config and just click Create

# Local setup (development)
1. Start Postgres
Assure you have `Docker` installed and running (with `docker compose` support).
```
cd scripts
./start-db-local.sh
```
Postgres will run on `localhost:1234`.
If you need to change ports, modify `db-docker-compose.yaml` file. If anything changes, assure to update `.env` file with the new values.

2. Build and run the server
```
cp .env.example .env
rustup override set 1.80.0
cargo run --bin server
```
It will listen on `localhost:3000`.
