{-# LANGUAGE DerivingStrategies #-}
{-# LANGUAGE GeneralizedNewtypeDeriving #-}

module Main where

import Colog.Core (LogAction, logStringStdout, (&>))
import Control.Exception.Safe (MonadCatch, MonadMask, MonadThrow, bracket,
                               catch, throwIO)
import Control.Monad.IO.Class (MonadIO (liftIO))
import Control.Monad.Reader (MonadReader (ask), ReaderT, runReaderT)
import Data.Text (Text)
import qualified Data.Text as T
import qualified Data.Text.IO as TIO
import GHC.IO.Exception (IOException)
import System.IO (Handle, IOMode (ReadMode), hClose, openFile)

-- House style for a new 2026 server: ReaderT Env IO with the logger and
-- config in the environment (the shape graphql-engine and HLS actually use,
-- and what RIO wraps). The structured logger is a co-log LogAction kept in
-- the env; it is not a global.

data Env = Env
  { envLogger :: LogAction IO String
  }

-- Section 1: the application monad.
newtype App a = App { unApp :: ReaderT Env IO a }
  deriving newtype (Functor, Applicative, Monad, MonadIO, MonadThrow,
                    MonadCatch, MonadMask, MonadReader Env)

runApp :: Env -> App a -> IO a
runApp env (App m) = runReaderT m env

-- Section 2: structural logging threaded through the env.
logInfo :: String -> App ()
logInfo msg = do
  lg <- envLogger <$> ask
  liftIO (msg &> lg)

-- Section 4: error discipline, two idioms both used by the reference
-- projects.
--
-- 1. A pure failure that is a normal outcome is carried in Either and lifted
--    at the boundary, the way postgrest lifts DB results with liftEither
--    after runExceptT at the handler edge (postgrest App.hs:160).
-- 2. A resource failure is an exception, delimited with bracket and caught
--    for its exact type only. Signals used here use throwIO, never throw, so
--    a pure computation cannot trigger them (safe-exceptions rule).

parsePort :: Text -> Either String Int
parsePort s = case reads (T.unpack s) of
  [(n, "")] | n >= 0 && n <= 65535 -> Right n
  _ -> Left ("bad port: " <> T.unpack s)

-- A required file. IOException is expected and is the only type caught.
readRequired :: FilePath -> App Text
readRequired path =
  bracket
    (liftIO (openFile path ReadMode))
    (\h -> liftIO (hClose h))
    (\h -> liftIO (TIO.hGetContents h))
    `catch` \e -> throwIO (userError (path <> ": " <> show (e :: IOException)))

startup :: FilePath -> Text -> App Int
startup msgFile portText = do
  logInfo "starting"
  msg <- readRequired msgFile
  logInfo ("config=" <> T.unpack msg)
  port <- case parsePort portText of
    Right p -> pure p
    Left err -> throwIO (userError err)
  pure port

main :: IO ()
main = do
  let env = Env { envLogger = logStringStdout }
  port <- runApp env (startup "config.txt" (T.pack "8080"))
  putStrLn ("app started on port " <> show port)
