-- Transport metadata is independent of complete business history and immutable pending packets.
CREATE TABLE assessment_sync_dirty (
    session_id TEXT PRIMARY KEY NOT NULL REFERENCES assessment_sessions(id),
    epoch INTEGER NOT NULL DEFAULT 1
);
INSERT INTO assessment_sync_dirty(session_id) SELECT id FROM assessment_sessions;
ALTER TABLE assessment_sync_state ADD COLUMN cached_epoch INTEGER NOT NULL DEFAULT 0;
ALTER TABLE assessment_sync_state ADD COLUMN cached_digest TEXT;
ALTER TABLE assessment_sync_state ADD COLUMN cached_pending_references INTEGER NOT NULL DEFAULT 0;
ALTER TABLE assessment_sync_state ADD COLUMN remote_revision INTEGER NOT NULL DEFAULT 0;
UPDATE assessment_sync_state SET remote_revision=COALESCE(json_extract(base_json,'$.revision'),0);
CREATE INDEX assessment_pending_refs ON assessment_sync_state(account_key,session_id) WHERE cached_pending_references>0;
CREATE INDEX assessment_answers_session ON assessment_answers(session_id,id);
CREATE INDEX assessment_corrections_answer ON assessment_corrections(answer_id,id);
CREATE INDEX assessment_collection_entry ON assessment_collection_links(entry_id);
CREATE INDEX sync_links_local_lookup ON sync_links(account_scope,entity_kind,local_id);
CREATE INDEX sync_outbox_compactable ON sync_outbox(account_scope,created_at)
    WHERE state IN ('sent','superseded') AND payload_json<>'{}';

CREATE TRIGGER sync_dirty_assessment_sessions_insert AFTER INSERT ON assessment_sessions
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.id,1 WHERE NEW.id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_sessions_update AFTER UPDATE ON assessment_sessions
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.id,1 WHERE NEW.id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_session_items_insert AFTER INSERT ON assessment_session_items
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_session_items_update AFTER UPDATE ON assessment_session_items
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_session_items_delete AFTER DELETE ON assessment_session_items
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT OLD.session_id,1 WHERE OLD.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_answers_insert AFTER INSERT ON assessment_answers
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_answers_update AFTER UPDATE ON assessment_answers
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_answers_delete AFTER DELETE ON assessment_answers
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT OLD.session_id,1 WHERE OLD.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_corrections_insert AFTER INSERT ON assessment_corrections
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_answers WHERE id=NEW.answer_id),1 WHERE (SELECT session_id FROM assessment_answers WHERE id=NEW.answer_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_corrections_update AFTER UPDATE ON assessment_corrections
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_answers WHERE id=NEW.answer_id),1 WHERE (SELECT session_id FROM assessment_answers WHERE id=NEW.answer_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_corrections_delete AFTER DELETE ON assessment_corrections
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_answers WHERE id=OLD.answer_id),1 WHERE (SELECT session_id FROM assessment_answers WHERE id=OLD.answer_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_report_versions_insert AFTER INSERT ON assessment_report_versions
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_report_versions_update AFTER UPDATE ON assessment_report_versions
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_report_versions_delete AFTER DELETE ON assessment_report_versions
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT OLD.session_id,1 WHERE OLD.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_collection_links_insert AFTER INSERT ON assessment_collection_links
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_session_items WHERE id=NEW.session_item_id),1 WHERE (SELECT session_id FROM assessment_session_items WHERE id=NEW.session_item_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_collection_links_update AFTER UPDATE ON assessment_collection_links
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_session_items WHERE id=NEW.session_item_id),1 WHERE (SELECT session_id FROM assessment_session_items WHERE id=NEW.session_item_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_dirty_assessment_collection_links_delete AFTER DELETE ON assessment_collection_links
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT (SELECT session_id FROM assessment_session_items WHERE id=OLD.session_item_id),1 WHERE (SELECT session_id FROM assessment_session_items WHERE id=OLD.session_item_id) IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_assessment_base_insert AFTER INSERT ON assessment_sync_state
WHEN NEW.base_json IS NOT NULL
BEGIN
    UPDATE assessment_sync_state SET remote_revision=COALESCE(json_extract(NEW.base_json,'$.revision'),0)
        WHERE account_key=NEW.account_key AND session_id=NEW.session_id;
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_assessment_base_update AFTER UPDATE ON assessment_sync_state
WHEN OLD.base_json IS NOT NEW.base_json
BEGIN
    UPDATE assessment_sync_state SET remote_revision=COALESCE(json_extract(NEW.base_json,'$.revision'),0)
        WHERE account_key=NEW.account_key AND session_id=NEW.session_id;
    INSERT INTO assessment_sync_dirty(session_id,epoch) SELECT NEW.session_id,1 WHERE NEW.session_id IS NOT NULL ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_assessment_alias_insert AFTER INSERT ON sync_links
WHEN NEW.entity_kind='entries'
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT DISTINCT i.session_id,1 FROM assessment_collection_links l
        JOIN assessment_session_items i ON i.id=l.session_item_id
        WHERE l.entry_id=NEW.local_id
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT session_id,1 FROM assessment_sync_state
        WHERE account_key=NEW.account_scope AND cached_pending_references>0
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_assessment_alias_update AFTER UPDATE ON sync_links
WHEN NEW.entity_kind='entries' OR OLD.entity_kind='entries'
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT DISTINCT i.session_id,1 FROM assessment_collection_links l
        JOIN assessment_session_items i ON i.id=l.session_item_id
        WHERE l.entry_id=NEW.local_id OR l.entry_id=OLD.local_id
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT session_id,1 FROM assessment_sync_state
        WHERE account_key IN (NEW.account_scope,OLD.account_scope) AND cached_pending_references>0
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;

CREATE TRIGGER sync_assessment_alias_delete AFTER DELETE ON sync_links
WHEN OLD.entity_kind='entries'
BEGIN
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT DISTINCT i.session_id,1 FROM assessment_collection_links l
        JOIN assessment_session_items i ON i.id=l.session_item_id
        WHERE l.entry_id=OLD.local_id
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
    INSERT INTO assessment_sync_dirty(session_id,epoch)
        SELECT session_id,1 FROM assessment_sync_state
        WHERE account_key=OLD.account_scope AND cached_pending_references>0
        ON CONFLICT(session_id) DO UPDATE SET epoch=epoch+1;
END;
