use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use color_eyre::eyre::{anyhow, bail};
use color_eyre::Result;
use diesel::{
    Connection, ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl, SqliteConnection,
};
use log::*;

use super::{BoxedDbIterResult, ReadableEnv, RuntimeEnv, WriteableEnv};
use crate::context::boot_data::mainnet_boot_data;
use crate::context::callbacks::{DefaultEnvCallbacks, RuntimeEnvCallbackHandler};
use crate::context::{
    Block, BlockCursor, BlockTransactionContext, Network, Runtime, StacksEnvPaths,
};
use crate::db::appdb::burnstate_db::{AppDbBurnStateWrapper, AsBurnStateDb};
use crate::db::appdb::headers_db::AsHeadersDb;
use crate::db::appdb::AppDb;
use crate::db::model;
use crate::db::schema::{self, _block_commits, _snapshots};
use crate::environments::stacks_node::db::stacks_headers_db::StacksHeadersDb;
use crate::types::BlockHeader;
use crate::{clarity, ok, stacks};

/// Holds the configuration of an [InstrumentedEnv].
pub struct InstrumentedEnvConfig {
    working_dir: PathBuf,
    readonly: bool,
    paths: StacksEnvPaths,
    runtime: Runtime,
    network: Network,
}

/// Holds the opened state of an [InstrumentedEnv].
pub struct InstrumentedEnvState {
    index_db_conn: RefCell<SqliteConnection>,
    chainstate: stacks::StacksChainState,
    clarity_db_conn: SqliteConnection,
    //sortition_db_conn: Rc<RefCell<SqliteConnection>>,
    //sortition_db: stacks::SortitionDB,
    burnstate_db: Box<dyn clarity::BurnStateDB>,
    headers_db: Box<dyn clarity::HeadersDB>,
}

/// This environment type is app-specific and will instrument all Clarity-related
/// operations. This environment can be used for comparisons.
pub struct InstrumentedEnv {
    id: i32,
    name: String,
    app_db: Rc<AppDb>,
    env_config: InstrumentedEnvConfig,
    env_state: Option<InstrumentedEnvState>,
    callbacks: Box<dyn RuntimeEnvCallbackHandler>,
}

impl InstrumentedEnv {
    /// Creates a new [InstrumentedEnv]. This method expects the provided
    /// `working_dir` to either be uninitialized or be using the same [Runtime]
    /// and [Network] configuration.
    pub fn new(
        id: i32,
        name: String,
        app_db: Rc<AppDb>,
        working_dir: PathBuf,
        runtime: Runtime,
        network: Network,
    ) -> Self {
        let paths = StacksEnvPaths::new(working_dir.clone());

        let env_config = InstrumentedEnvConfig {
            working_dir,
            readonly: false,
            paths,
            runtime,
            network,
        };

        Self {
            id,
            name,
            app_db,
            env_config,
            env_state: None,
            callbacks: Box::<DefaultEnvCallbacks>::default(),
        }
    }

    fn readonly(&mut self, readonly: bool) {
        self.env_config.readonly = readonly;
    }

    /// Attempts to retrieve the [InstrumentedEnvState] for this environment. Will
    /// return an error if [RuntimeEnv::open] has not been called.
    fn get_env_state(&self) -> Result<&InstrumentedEnvState> {
        let state = self
            .env_state
            .as_ref()
            .ok_or(anyhow!("[{}] environment has not been opened", self.name))?;

        Ok(state)
    }

    /// Attempts to retrieve the [InstrumentedEnvState] for this environment as a
    /// mutable reference. Will return an error if [RuntimeEnv::open] has not been called.
    fn get_env_state_mut(&mut self) -> Result<&mut InstrumentedEnvState> {
        let state = self
            .env_state
            .as_mut()
            .ok_or(anyhow!("[{}] environment has not been opened", self.name))?;

        Ok(state)
    }

    /// Retrieve all block headers from the underlying storage.
    fn block_headers(&self) -> Result<Vec<BlockHeader>> {
        // Get our state
        let state = self
            .env_state
            .as_ref()
            .ok_or(anyhow!("environment has not been opened"))?;

        // Retrieve the tip.
        self.callbacks.get_chain_tip_start(self);
        let tip = schema::_block_headers::table
            .order_by(schema::_block_headers::block_height.desc())
            .limit(1)
            .get_result::<model::BlockHeader>(&mut *state.index_db_conn.borrow_mut())?;
        // TODO: Handle when there is no tip (chain uninitialized).
        self.callbacks
            .get_chain_tip_finish(self, tip.block_height as u32);
        let mut current_block = Some(tip);

        // Vec for holding the headers we run into. This will initially be
        // in reverse order (from tip to genesis) - we reverse it later.
        let mut headers: Vec<BlockHeader> = Vec::new();

        // Walk backwards from tip to genesis, following the canonical fork. We
        // do this so that we don't follow orphaned blocks/forks.
        while let Some(block) = current_block {
            let block_parent = schema::_block_headers::table
                .filter(schema::_block_headers::index_block_hash.eq(&block.parent_block_id))
                .get_result::<model::BlockHeader>(&mut *state.index_db_conn.borrow_mut())
                .optional()?;

            headers.push(block.try_into()?);
            self.callbacks.load_block_headers_iter(self, headers.len());

            current_block = block_parent;
        }

        // Reverse the vec so that it is in block-ascending order.
        headers.reverse();

        debug!("first block: {:?}", headers[0]);
        debug!("tip: {:?}", headers[headers.len() - 1]);
        debug!("retrieved {} block headers", headers.len());

        self.callbacks
            .load_block_headers_finish(self, headers.len());
        Ok(headers)
    }
}

impl AsHeadersDb for InstrumentedEnv {
    fn as_headers_db(&self) -> Result<&dyn clarity::HeadersDB> {
        let state = self.get_env_state()?;
        Ok(&*state.headers_db)
    }
}

impl AsBurnStateDb for InstrumentedEnv {
    fn as_burnstate_db(&self) -> Result<&dyn clarity::BurnStateDB> {
        let state = self.get_env_state()?;
        Ok(&*state.burnstate_db)
    }
}

/// Implementation of [RuntimeEnv] for [InstrumentedEnv].
impl RuntimeEnv for InstrumentedEnv {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_readonly(&self) -> bool {
        self.env_config.readonly
    }

    fn is_open(&self) -> bool {
        self.env_state.is_some()
    }

    fn open(&mut self) -> Result<()> {
        let name = &self.name;
        let paths = &self.env_config.paths;
        let network = &self.env_config.network;

        info!("[{name}] opening environment...");
        self.callbacks
            .env_open_start(self, &self.env_config.working_dir);
        paths.print(name);

        // Setup our options for the Marf.
        let mut marf_opts = stacks::MARFOpenOpts::default();
        marf_opts.external_blobs = true;

        // Setup our boot data to be used if the chainstate hasn't been initialized yet.
        let mut boot_data = if network.is_mainnet() {
            mainnet_boot_data()
        } else {
            todo!("testnet not yet supported")
        };

        debug!("initializing chainstate");
        self.callbacks
            .open_chainstate_start(self, &paths.chainstate_dir);
        let (chainstate, _) = stacks::StacksChainState::open_and_exec(
            network.is_mainnet(),
            1,
            &paths.chainstate_dir.display().to_string(),
            Some(&mut boot_data),
            Some(marf_opts.clone()),
        )?;
        self.callbacks.open_chainstate_finish(self);
        info!("[{name}] chainstate initialized.");

        debug!("[{name}] loading index db...");
        self.callbacks
            .open_index_db_start(self, &paths.index_db_path);
        let index_db_conn =
            SqliteConnection::establish(&paths.index_db_path.display().to_string())?;
        self.callbacks.open_index_db_finish(self);
        info!("[{name}] successfully connected to index db");

        // Open the Clarity sqlite db
        debug!("[{name}] loading clarity db...");
        self.callbacks
            .open_clarity_db_start(self, &paths.clarity_db_path);
        let clarity_db_conn =
            SqliteConnection::establish(&paths.clarity_db_path.display().to_string())?;
        self.callbacks.open_clarity_db_finish(self);
        info!("[{name}] successfully connected to clarity db");

        let burnstate_db =
            AppDbBurnStateWrapper::new(self.id, self.app_db.clone(), boot_data.pox_constants);

        //let headers_db = AppDbHeadersWrapper::new(self.id, self.app_db.clone());

        // Open the burnstate db
        /*
        let burnstate_db = StacksBurnStateDb::new(
            &paths.sortition_db_path,
            boot_data.pox_constants,
        )?;
        */

        // Open the headers db
        let headers_db = StacksHeadersDb::new(&paths.index_db_path)?;

        let state = InstrumentedEnvState {
            chainstate,
            index_db_conn: RefCell::new(index_db_conn),
            clarity_db_conn,
            burnstate_db: Box::new(burnstate_db),
            headers_db: Box::new(headers_db),
        };

        self.env_state = Some(state);

        ok!()
    }

    fn id(&self) -> i32 {
        self.id
    }
}

/// Implementation of [ReadableEnv] for [InstrumentedEnv].
impl ReadableEnv for InstrumentedEnv {
    fn blocks(&self) -> Result<BlockCursor> {
        let headers = self.block_headers()?;
        let cursor = BlockCursor::new(&self.env_config.paths.blocks_dir, headers);
        Ok(cursor)
    }

    fn snapshots(&self) -> BoxedDbIterResult<crate::types::Snapshot> {
        let result = self
            .app_db
            .stream_results::<model::Snapshot, crate::types::Snapshot, _>(_snapshots::table, 100);

        Ok(Box::new(result))
    }

    fn snapshot_count(&self) -> Result<usize> {
        self.app_db.snapshot_count(self.id)
    }

    fn block_commits(&self) -> Result<Box<dyn Iterator<Item = Result<crate::types::BlockCommit>>>> {
        let result = self
            .app_db
            .stream_results::<model::BlockCommit, crate::types::BlockCommit, _>(
                _block_commits::table,
                1000,
            );

        Ok(Box::new(result))
    }

    fn block_commit_count(&self) -> Result<usize> {
        self.app_db.block_commit_count(self.id)
    }

    fn ast_rules(&self) -> BoxedDbIterResult<crate::types::AstRuleHeight> {
        todo!()
    }

    fn ast_rule_count(&self) -> Result<usize> {
        self.app_db.ast_rule_count(self.id)
    }

    fn epochs(&self) -> BoxedDbIterResult<crate::types::Epoch> {
        todo!()
    }

    fn epoch_count(&self) -> Result<usize> {
        self.app_db.epoch_count(self.id)
    }

    fn block_headers(&self) -> BoxedDbIterResult<crate::types::BlockHeader> {
        todo!()
    }

    fn block_header_count(&self) -> Result<usize> {
        todo!()
    }

    fn paths(&self) -> StacksEnvPaths {
        self.env_config.paths.clone()
    }
}

/// Implementation of [WriteableEnv] for [InstrumentedEnv].
impl WriteableEnv for InstrumentedEnv {
    fn block_begin(&mut self, block: &crate::context::Block) -> Result<()> {
        if self.is_readonly() {
            bail!("[{}] environment is read-only.", self.name);
        }

        debug!("block: {block:?}");

        // Insert this block into the app database.
        debug!("creating block in app datastore");
        self.app_db.insert_block(
            self.id,
            block.block_height()? as i32,
            block.block_hash()?.as_bytes(),
            block.index_block_hash()?,
        )?;

        let state = self.get_env_state_mut()?;

        match block {
            Block::Genesis(inner) => {
                info!(
                    "Reached GENESIS block: {}",
                    &inner.header.index_block_hash.to_hex()
                );
                info!("Genesis block has already been processed as a part of boot init - continuing...");
            }
            Block::Regular(inner) => {
                info!(
                    "beginning regular block: {}",
                    &inner.header.index_block_hash.to_hex()
                );

                let parent_consensus_hash = &inner.parent_header.consensus_hash;
                let parent_block_hash = &inner.parent_header.block_hash;
                let new_consensus_hash = &inner.header.consensus_hash;
                let new_block_hash = &inner.header.block_hash;

                debug!("parent_consensus_hash: {}, parent_block: {}, new_consensus_hash: {}, new_block: {}",
                    parent_consensus_hash.to_hex(),
                    parent_block_hash.to_hex(),
                    new_consensus_hash.to_hex(),
                    new_block_hash.to_hex()
                );

                let clarity_tx = state.chainstate.block_begin(
                    &*state.burnstate_db,
                    parent_consensus_hash,
                    parent_block_hash,
                    new_consensus_hash,
                    new_block_hash,
                );

                // Transaction processing here

                info!("committing regular block");
                clarity_tx.commit_to_block(new_consensus_hash, new_block_hash);
            }
        };

        ok!()
    }

    fn block_commit(
        &mut self,
        block_tx_ctx: BlockTransactionContext,
    ) -> Result<clarity::LimitedCostTracker> {
        if let BlockTransactionContext::Regular(_ctx) = block_tx_ctx {
            //clarity_tx_conn.commit();
            //let cost_tracker = clarity_block_conn.commit_to_block(&stacks_block_id);
            //chainstate_tx.commit()?;
        } else {
            bail!("Cannot commit genesis block as it has already been statically loaded from boot data.")
        }

        todo!()
    }

    /// Imports chainstate from the provided source environment into this environment.
    fn import_chainstate(&self, source: &dyn ReadableEnv) -> Result<()> {
        debug!(
            "importing block headers from '{}' into '{}'...",
            source.name(),
            self.name()
        );
        let src_block_headers_iter = source.block_headers()?;

        let mut headers_db = StacksHeadersDb::new(&self.env_config.paths.index_db_path)?;
        headers_db.import_block_headers(src_block_headers_iter, Some(self.id()))?;

        ok!()
    }

    /// Imports burnstate from the provided source environment into this environment.
    fn import_burnstate(&self, source: &dyn ReadableEnv) -> Result<()> {
        debug!(
            "importing snapshots from '{}' into '{}'...",
            source.name(),
            self.name()
        );
        let src_snapshots_iter = source.snapshots()?;
        self.app_db
            .batch()
            .import_snapshots(src_snapshots_iter, Some(self.id()))?;

        debug!(
            "importing block commits from '{}' into '{}'...",
            source.name(),
            self.name()
        );
        let src_block_commits_iter = source.block_commits()?;
        self.app_db
            .batch()
            .import_block_commits(src_block_commits_iter, Some(self.id()))?;

        debug!(
            "importing AST rules from '{}' into '{}'...",
            source.name(),
            self.name()
        );
        let src_ast_rules_iter = source.ast_rules()?;
        self.app_db
            .batch()
            .import_ast_rules(src_ast_rules_iter, Some(self.id()))?;

        debug!(
            "importing epochs from '{}' into '{}'...",
            source.name(),
            self.name()
        );
        let src_epochs_iter = source.epochs()?;
        self.app_db
            .batch()
            .import_epochs(src_epochs_iter, Some(self.id()))?;

        ok!()
    }
}
