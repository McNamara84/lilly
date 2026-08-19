group "e2e" {
  targets = ["backend", "frontend"]
}

target "backend" {
  context    = "."
  dockerfile = "backend/Dockerfile"
  target     = "development"
  tags       = ["lilly-backend:e2e"]
  cache-from = ["type=gha,scope=e2e-backend"]
}

target "frontend" {
  context    = "frontend"
  dockerfile = "Dockerfile"
  tags       = ["lilly-frontend:e2e"]
  cache-from = ["type=gha,scope=e2e-frontend"]
}
