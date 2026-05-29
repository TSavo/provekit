package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
)

type rpcRequest struct {
	Method string `json:"method"`
}

func main() {
	if err := run(); err != nil {
		fmt.Fprintf(os.Stderr, "go-bind-rpc: %v\n", err)
		os.Exit(1)
	}
}

func run() error {
	projectRoot, err := os.Getwd()
	if err != nil {
		return err
	}
	repoRoot := filepath.Clean(filepath.Join(projectRoot, "..", ".."))

	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 1024*1024), 16*1024*1024)

	var child *exec.Cmd
	var childStdin io.WriteCloser
	for scanner.Scan() {
		line := append([]byte(nil), scanner.Bytes()...)
		var req rpcRequest
		if err := json.Unmarshal(line, &req); err != nil {
			return err
		}
		if child == nil {
			child, childStdin, err = startKit(repoRoot, req.Method)
			if err != nil {
				return err
			}
		}
		if _, err := childStdin.Write(append(line, '\n')); err != nil {
			return err
		}
		if req.Method == "shutdown" {
			if err := childStdin.Close(); err != nil {
				return err
			}
			if err := child.Wait(); err != nil {
				return err
			}
			child = nil
			childStdin = nil
		}
	}
	if err := scanner.Err(); err != nil {
		return err
	}
	if childStdin != nil {
		_ = childStdin.Close()
	}
	if child != nil {
		return child.Wait()
	}
	return nil
}

func startKit(repoRoot, method string) (*exec.Cmd, io.WriteCloser, error) {
	kitDir := filepath.Join(repoRoot, "implementations", "go")
	args := []string{"run", "./cmd/provekit-lift-go-verify", "--rpc"}
	if method == "provekit.plugin.recognize" {
		kitDir = filepath.Join(kitDir, "provekit-lift-go")
		args = []string{"run", "./cmd/provekit-lift-go", "--rpc"}
	}

	cmd := exec.Command("go", args...)
	cmd.Dir = kitDir
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	stdin, err := cmd.StdinPipe()
	if err != nil {
		return nil, nil, err
	}
	if err := cmd.Start(); err != nil {
		_ = stdin.Close()
		return nil, nil, err
	}
	return cmd, stdin, nil
}
