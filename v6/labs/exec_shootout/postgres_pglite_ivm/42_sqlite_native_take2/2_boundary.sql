PRAGMA recursive_triggers=ON;
CREATE TABLE source(id INTEGER PRIMARY KEY,k INTEGER UNIQUE,v INTEGER CHECK(v>=0));
CREATE VIRTUAL TABLE maintained USING take2;
CREATE TRIGGER source_ai AFTER INSERT ON source BEGIN
  INSERT INTO maintained(id,k,v,op) VALUES(NEW.id,NEW.k,NEW.v,1);
END;
CREATE TRIGGER source_ad AFTER DELETE ON source BEGIN
  INSERT INTO maintained(op,old_id,old_k,old_v) VALUES(2,OLD.id,OLD.k,OLD.v);
END;
CREATE TRIGGER source_au AFTER UPDATE ON source BEGIN
  INSERT INTO maintained(id,k,v,op,old_id,old_k,old_v)
  VALUES(NEW.id,NEW.k,NEW.v,3,OLD.id,OLD.k,OLD.v);
END;
