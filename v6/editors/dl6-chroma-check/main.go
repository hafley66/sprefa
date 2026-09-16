// Tokenises editors/dl6.fixture.dl6 with editors/dl6.chroma.xml and fails when
// the lexer drops a class the .dl6 surface needs.
package main

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/alecthomas/chroma/v2"
)

var want = []chroma.TokenType{
	chroma.CommentSingle, chroma.KeywordDeclaration, chroma.KeywordType,
	chroma.NameBuiltin, chroma.KeywordReserved, chroma.NameNamespace,
	chroma.NameFunction, chroma.NameAttribute, chroma.NameVariable,
	chroma.NameDecorator, chroma.Operator, chroma.LiteralStringSymbol,
	chroma.LiteralStringDouble, chroma.LiteralStringBacktick,
	chroma.LiteralStringInterpol, chroma.LiteralNumberInteger,
	chroma.LiteralNumberFloat, chroma.Punctuation,
}

func main() {
	dir := "."
	if len(os.Args) > 1 {
		dir = os.Args[1]
	}
	lexer := chroma.MustNewXMLLexer(os.DirFS(dir), "dl6.chroma.xml")
	source, err := os.ReadFile(filepath.Join(dir, "dl6.fixture.dl6"))
	if err != nil {
		panic(err)
	}
	iterator, err := lexer.Tokenise(nil, string(source))
	if err != nil {
		panic(err)
	}
	tokens := iterator.Tokens()
	counts := map[chroma.TokenType]int{}
	for _, token := range tokens {
		counts[token.Type]++
	}
	missing := 0
	for _, tokenType := range want {
		fmt.Printf("%-24s %d\n", tokenType, counts[tokenType])
		if counts[tokenType] == 0 {
			missing++
		}
	}
	if bad := counts[chroma.Error]; bad > 0 {
		fmt.Printf("FAIL %d Error tokens\n", bad)
		os.Exit(1)
	}
	if missing > 0 {
		fmt.Printf("FAIL %d token classes never fired\n", missing)
		os.Exit(1)
	}
	fmt.Printf("OK %d token classes, %d tokens\n", len(want), len(tokens))
}
