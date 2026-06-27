package run_test_failed

import (
	"fmt"
	"os"
	"testing"
)

func TestPassed(t *testing.T) {
	fmt.Println("x")
	fmt.Println("y")
}

func TestMain(m *testing.M) {
	os.Exit(0)
}
