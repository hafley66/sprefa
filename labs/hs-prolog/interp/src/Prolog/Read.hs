-- Surface reader for the fixture clauses: tokenizer plus an operator
-- precedence parser (SWI term order). Parsec is used only for the tokenizer;
-- the solver is Prolog.Solve. See REPORT.md for the build-vs-buy note.

module Prolog.Read
  ( Program(..)
  , readProgram
  , parseQuery
  , toGoalList
  ) where

import Data.Char (isDigit, isAlphaNum, isSpace, isUpper, isLower, isAlpha)
import Prolog.Solve (Clause(..))
import Prolog.Term (Term(..), Var(..), mkList, nilList)

data Program = Program
  { programClauses :: [Clause]
  , programTabled  :: [(String, Int)]
  , programChecks  :: [(String, Term)]
  }

-- tokenizer ---------------------------------------------------------------

data Token
  = TVarT String
  | TAtomT String
  | TIntT Integer
  | TStrT String
  | TSymT String
  | TLParen
  | TRParen
  | TLBrack
  | TRBrack
  | TComma
  | TFullStop
  deriving (Show, Eq)

isSymbolChar :: Char -> Bool
isSymbolChar c = c `elem` ("+-*/\\=<>@:~?|;&#^" :: String)

-- Position pair needed by the parser's anonymous-variable counter.
lexPro :: String -> Either String [Token]
lexPro input = go input
  where
    go [] = Right []
    go (c : rest)
      | isSpace c = go rest
      | c == '%' = go (dropWhile (/= '\n') rest)
      | otherwise = do
          (token, rest') <- lexOne c rest
          tokens <- go rest'
          pure (token : tokens)

    lexOne c rest = case c of
      '(' -> Right (TLParen, rest)
      ')' -> Right (TRParen, rest)
      '[' -> Right (TLBrack, rest)
      ']' -> Right (TRBrack, rest)
      ',' -> Right (TComma, rest)
      '.' -> Right (TFullStop, rest)
      '!' -> Right (TSymT "!", rest)
      '\'' -> quoted rest
      '"' -> stringLit rest
      _ | isDigit c -> integerLit c rest
        | c == '_' || isUpper c -> variable c rest
        | isLower c || isAlpha c -> atomLit c rest
        | isSymbolChar c -> symbol c rest
        | otherwise -> Left ("lex: bad char " ++ show c)

    quoted src = do
      (acc, rest) <- untilQuote src
      pure (TAtomT acc, rest)
      where
        untilQuote [] = Left "unterminated quoted atom"
        untilQuote ('\'' : '\\' : e : rest) = do
          (acc, rest') <- untilQuote rest
          pure (e : acc, rest')
        untilQuote ('\'' : rest) = Right ("", rest)
        untilQuote (c : rest) = do
          (acc, rest') <- untilQuote rest
          pure (c : acc, rest')

    stringLit src = do
      (acc, rest) <- untilStr src
      pure (TStrT acc, rest)
      where
        untilStr [] = Left "unterminated string"
        untilStr ('"' : rest) = Right ("", rest)
        untilStr ('\\' : c : rest) = do
          (acc, rest') <- untilStr rest
          pure (c : acc, rest')
        untilStr (c : rest) = do
          (acc, rest') <- untilStr rest
          pure (c : acc, rest')

    integerLit first rest =
      let (ds, rest') = span isDigit rest
      in Right (TIntT (read (first : ds)), rest')

    variable first rest =
      let (ds, rest') = span (\c -> isAlphaNum c || c == '_') rest
      in Right (TVarT (first : ds), rest')

    atomLit first rest =
      let (ds, rest') = span (\c -> isAlphaNum c || c == '_') rest
      in Right (TAtomT (first : ds), rest')

    symbol first rest =
      let (ds, rest') = span isSymbolChar rest
      in Right (TSymT (first : ds), rest')

-- parser ---------------------------------------------------------------

-- Simple hand-rolled parser over a token list, carrying an anonymous-variable
-- counter. No general backtracking combinators needed: the grammar inspects the
-- next token before deciding.
newtype P a = P { unP :: (Int, [Token]) -> Either String (a, Int, [Token]) }

instance Functor P where
  fmap f (P g) = P $ \s -> do
    (a, i, rest) <- g s
    pure (f a, i, rest)

instance Applicative P where
  pure a = P $ \st -> Right (a, fst st, snd st)
  P f <*> P g = P $ \st -> do
    (h, i, rest) <- f st
    (a, i', rest') <- g (i, rest)
    pure (h a, i', rest')

instance Monad P where
  P g >>= f = P $ \st -> do
    (a, i, rest) <- g st
    unP (f a) (i, rest)

instance MonadFail P where
  fail msg = P $ \_ -> Left msg

peek :: P Token
peek = P $ \(i, toks) -> case toks of
  [] -> Left "unexpected end of input"
  (t : _) -> Right (t, i, toks)

peekSoft :: P (Maybe Token)
peekSoft = P $ \(i, toks) -> case toks of
  [] -> Right (Nothing, i, toks)
  (t : _) -> Right (Just t, i, toks)

next :: P Token
next = P $ \(i, toks) -> case toks of
  [] -> Left "unexpected end of input"
  (t : rest) -> Right (t, i, rest)

expect :: Token -> P ()
expect t = do
  t' <- next
  if t' == t then pure () else fail ("expected " ++ show t ++ " got " ++ show t')

freshAnon :: P Term
freshAnon = P $ \(i, toks) -> Right (TVar (Var ("_g" ++ show i) 0), i + 1, toks)

runP :: P a -> [Token] -> Either String a
runP p toks = do
  (a, _, rest) <- unP p (0, toks)
  case rest of
    [] -> Right a
    _ -> Left ("parse: leftover tokens " ++ show rest)

-- operator tables, in SWI precedence numbers (higher number binds looser)

infixOp :: String -> Maybe Int
infixOp name = case name of
  ":-" -> Just 1200
  "-->" -> Just 1200
  ";" -> Just 1100
  "|" -> Just 1105
  "->" -> Just 1050
  "," -> Just 1000
  "=" -> Just 700
  "==" -> Just 700
  "=@=" -> Just 700
  "is" -> Just 700
  "<" -> Just 700
  ":" -> Just 600
  "+" -> Just 500
  "-" -> Just 500
  "*" -> Just 400
  "/" -> Just 400
  _ -> Nothing

prefixOp :: String -> Maybe Int
prefixOp "\\+" = Just 900
prefixOp _ = Nothing

-- parseTerm bound termOps: parse a term, consuming binary operators with SWI
-- precedence <= bound that are not in termOps (which act as delimiters for that
-- context). The right operand of an (xfy) operator is parsed at the operator's
-- own precedence, giving SWI's right-associative nesting. Contexts:
--   functor arguments: termOps = [","], so "," and ")" delimit arguments.
--   list elements:     termOps = [",", "|"].
--   full term/group:   termOps = [].
parseTerm :: Int -> [String] -> P Term
parseTerm bound termOps = do
  left <- parsePrimary bound termOps
  rest bound left termOps

rest :: Int -> Term -> [String] -> P Term
rest bound left termOps = do
  t <- peekSoft
  case t of
    Just tok
      | Just (name, p) <- binaryOp tok
      , p <= bound
      , name `notElem` termOps -> do
          _ <- next
          right <- parseTerm p termOps
          rest bound (TStruct name [left, right]) termOps
    _ -> pure left

-- The binary interpretation of a token: an operator symbol, or the comma, whose
-- strictness is 1000.
binaryOp :: Token -> Maybe (String, Int)
binaryOp TComma = Just (",", 1000)
binaryOp (TSymT name) = (,) name <$> infixOp name
binaryOp _ = Nothing

parsePrimary :: Int -> [String] -> P Term
parsePrimary bound termOps = do
  t <- peek
  case t of
    TLParen -> do
      _ <- next
      inner <- parseTerm 1200 []
      expect TRParen
      pure inner
    TLBrack -> parseList termOps
    TVarT "_" -> do
      _ <- next
      freshAnon
    TVarT name -> do
      _ <- next
      pure (TVar (Var name 0))
    TAtomT name -> do
      _ <- next
      applyFunctor name termOps
    TIntT n -> do
      _ <- next
      pure (TInt n)
    TStrT s -> do
      _ <- next
      pure (mkList (map (TInt . fromIntegral . fromEnum) s))
    TSymT "!" -> do
      _ <- next
      pure (TAtom "!")
    TSymT "[]" -> do
      _ <- next
      pure nilList
    TSymT name
      | Just p <- prefixOp name -> do
          _ <- next
          arg <- parseTerm p termOps
          pure (TStruct name [arg])
    _ -> fail ("unexpected token " ++ show t)

applyFunctor :: String -> [String] -> P Term
applyFunctor name termOps = do
  t <- peek
  case t of
    TLParen -> do
      _ <- next
      args <- commaArgs termOps
      expect TRParen
      pure (TStruct name args)
    _ -> pure (TAtom name)

commaArgs :: [String] -> P [Term]
commaArgs termOps = do
  first <- parseTerm 1200 ("," : termOps)
  t <- peek
  case t of
    TComma -> do
      _ <- next
      rest <- commaArgs termOps
      pure (first : rest)
    _ -> pure [first]

parseList :: [String] -> P Term
parseList outerOps = do
  expect TLBrack
  t <- peek
  case t of
    TRBrack -> do
      _ <- next
      pure nilList
    _ -> do
      (items, mTail) <- listItems outerOps
      expect TRBrack
      let base = case mTail of
            Nothing -> nilList
            Just tail_ -> tail_
      pure (foldr (\h r -> TStruct ":" [h, r]) base items)

listItems :: [String] -> P ([Term], Maybe Term)
listItems outerOps = do
  first <- parseTerm 1200 [",", "|"]
  t <- peek
  case t of
    TComma -> do
      _ <- next
      (rest, mTail) <- listItems outerOps
      pure (first : rest, mTail)
    TSymT "|" -> do
      _ <- next
      tail_ <- parseTerm 1200 outerOps
      pure ([first], Just tail_)
    _ -> pure ([first], Nothing)

-- clause assembly -------------------------------------------------------

toGoalList :: Term -> [Term]
toGoalList (TStruct "," [a, b]) = toGoalList a ++ toGoalList b
toGoalList term = [term]

flattenAnd :: Term -> [Term]
flattenAnd = toGoalList

-- top-level item loop ----------------------------------------------------

-- Parses one clause or directive. Directives (use_module, table) are consumed
-- and, for table, recorded.
parseItems :: P ([Clause], [(String, Int)])
parseItems = do
  t <- peekSoft
  case t of
    Nothing -> pure ([], [])
    Just TFullStop -> do
      _ <- next
      pure ([], [])
    Just _ -> do
      (thisClauses, thisTabled) <- parseItem
      (moreClauses, moreTabled) <- parseItems
      pure (thisClauses ++ moreClauses, thisTabled ++ moreTabled)

parseItem :: P ([Clause], [(String, Int)])
parseItem = do
  t <- peek
  case t of
    TSymT ":-" -> do
      _ <- next
      t2 <- peek
      case t2 of
        TAtomT "use_module" -> do
          skipUntilStop
          pure ([], [])
        TAtomT "table" -> do
          _ <- next
          tabled <- tableList
          expect TFullStop
          pure ([], tabled)
        _ -> do
          body <- parseTerm 1200 []
          expect TFullStop
          pure ([], [])
    _ -> do
      term <- parseTerm 1200 []
      expect TFullStop
      pure (classify term, [])

tableList :: P [(String, Int)]
tableList = do
  (name, arity) <- oneTableEntry
  t <- peek
  case t of
    TComma -> do
      _ <- next
      rest <- tableList
      pure ((name, arity) : rest)
    _ -> pure [(name, arity)]

oneTableEntry :: P (String, Int)
oneTableEntry = do
  t <- next
  case t of
    TAtomT name -> do
      t2 <- next
      case t2 of
        TSymT "/" -> do
          t3 <- next
          case t3 of
            TIntT arity -> pure (name, fromIntegral arity)
            _ -> fail "table: expected arity"
        _ -> fail "table: expected /arity"
    _ -> fail "table: expected functor name"

skipUntilStop :: P ()
skipUntilStop = do
  t <- next
  case t of
    TFullStop -> pure ()
    _ -> skipUntilStop

classify :: Term -> [Clause]
classify (TStruct ":-" [head_, body]) =
  [Clause head_ (flattenAnd body)]
classify (TStruct ":-" [_]) = []
classify (TStruct "-->" [_, _]) = []
classify term = [Clause term []]

-- program entry ----------------------------------------------------------

readProgram :: String -> Either String Program
readProgram src = do
  toks <- lexPro src
  (clauses, tabled) <- runP parseItems toks
  let checks = [ (name, goal)
               | Clause (TStruct "check" [TAtom name, goal]) [] <- clauses ]
  pure (Program clauses tabled checks)

parseQuery :: String -> Either String Term
parseQuery src = do
  toks <- lexPro src
  runP (parseTerm 1200 []) toks
