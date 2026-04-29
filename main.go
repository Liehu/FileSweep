package main

import "filesweep/cmd"

func main() {
	cmd.StaticFiles = staticFiles
	cmd.Execute()
}
