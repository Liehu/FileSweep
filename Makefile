.PHONY: build build-all test lint clean dev-backend dev-frontend

BINARY_NAME=filesweep
DIST_DIR=dist

build:
	go build -ldflags="-s -w" -o $(DIST_DIR)/$(BINARY_NAME).exe .

build-all:
	GOOS=windows GOARCH=amd64 go build -ldflags="-s -w" -o $(DIST_DIR)/filesweep-windows-amd64.exe .
	GOOS=linux GOARCH=amd64 go build -ldflags="-s -w" -o $(DIST_DIR)/filesweep-linux-amd64 .
	GOOS=darwin GOARCH=amd64 go build -ldflags="-s -w" -o $(DIST_DIR)/filesweep-darwin-amd64 .
	GOOS=darwin GOARCH=arm64 go build -ldflags="-s -w" -o $(DIST_DIR)/filesweep-darwin-arm64 .

test:
	go test ./tests/ -v

lint:
	golangci-lint run ./...

clean:
	rm -rf $(DIST_DIR)

dev-backend:
	go run main.go serve --port 8080

dev-frontend:
	cd frontend && npm run dev
