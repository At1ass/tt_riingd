use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use dashmap::DashMap;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing::info;

// ============================================================================
// SERVICE TYPES
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceType {
    Monitoring,
    FanColor,
    Broadcast,
    Coordinator,
}

// ============================================================================
// SEALED TRAITS FOR COMPILE-TIME ROUTING
// ============================================================================

mod private {
    pub trait Sealed {}
}

pub trait SingleTarget: private::Sealed {
    const TARGET: ServiceType;
}

pub trait MultipleTarget: private::Sealed {
    const TARGETS: &'static [ServiceType];
}

// ============================================================================
// QUERY TYPES (SINGLE TARGET)
// ============================================================================

#[derive(Debug, Clone)]
pub struct GetTemperatureQuery {
    pub device_id: String,
}

#[derive(Debug, Clone)]
pub struct GetAllSensorDataQuery;

#[derive(Debug, Clone)]
pub struct GetColorQuery {
    pub device_id: String,
}

// Single target implementations
impl private::Sealed for GetTemperatureQuery {}
impl SingleTarget for GetTemperatureQuery {
    const TARGET: ServiceType = ServiceType::Monitoring;
}

impl private::Sealed for GetAllSensorDataQuery {}
impl SingleTarget for GetAllSensorDataQuery {
    const TARGET: ServiceType = ServiceType::Monitoring;
}

impl private::Sealed for GetColorQuery {}
impl SingleTarget for GetColorQuery {
    const TARGET: ServiceType = ServiceType::FanColor;
}

// ============================================================================
// QUERY TYPES (MULTIPLE TARGET)
// ============================================================================

#[derive(Debug, Clone)]
pub struct HealthCheckQuery;

impl private::Sealed for HealthCheckQuery {}
impl MultipleTarget for HealthCheckQuery {
    const TARGETS: &'static [ServiceType] = &[
        ServiceType::Monitoring,
        ServiceType::FanColor,
        ServiceType::Broadcast,
    ];
}

// ============================================================================
// COMMAND TYPES (SINGLE TARGET)
// ============================================================================

#[derive(Debug, Clone)]
pub struct SetColorCommand {
    pub device_id: String,
    pub color: String,
}

#[derive(Debug, Clone)]
pub struct SetTemperatureCommand {
    pub device_id: String,
    pub temperature: f32,
}

// Single target implementations
impl private::Sealed for SetColorCommand {}
impl SingleTarget for SetColorCommand {
    const TARGET: ServiceType = ServiceType::FanColor;
}

impl private::Sealed for SetTemperatureCommand {}
impl SingleTarget for SetTemperatureCommand {
    const TARGET: ServiceType = ServiceType::Monitoring;
}

// ============================================================================
// COMMAND TYPES (MULTIPLE TARGET)
// ============================================================================

#[derive(Debug, Clone)]
pub struct PrepareConfigUpdateCommand {
    pub transaction_id: u64,
}

#[derive(Debug, Clone)]
pub struct PrepareShutdownCommand;

#[derive(Debug, Clone)]
pub struct CommitConfigUpdateCommand {
    pub transaction_id: u64,
}

#[derive(Debug, Clone)]
pub struct RollbackConfigUpdateCommand {
    pub transaction_id: u64,
}

impl private::Sealed for PrepareConfigUpdateCommand {}
impl MultipleTarget for PrepareConfigUpdateCommand {
    const TARGETS: &'static [ServiceType] = &[
        ServiceType::Monitoring,
        ServiceType::FanColor,
        ServiceType::Broadcast,
    ];
}

impl private::Sealed for PrepareShutdownCommand {}
impl MultipleTarget for PrepareShutdownCommand {
    const TARGETS: &'static [ServiceType] = &[
        ServiceType::Monitoring,
        ServiceType::FanColor,
        ServiceType::Broadcast,
    ];
}

impl private::Sealed for CommitConfigUpdateCommand {}
impl MultipleTarget for CommitConfigUpdateCommand {
    const TARGETS: &'static [ServiceType] = &[
        ServiceType::Monitoring,
        ServiceType::FanColor,
        ServiceType::Broadcast,
    ];
}

impl private::Sealed for RollbackConfigUpdateCommand {}
impl MultipleTarget for RollbackConfigUpdateCommand {
    const TARGETS: &'static [ServiceType] = &[
        ServiceType::Monitoring,
        ServiceType::FanColor,
        ServiceType::Broadcast,
    ];
}

// ============================================================================
// REQUEST PAYLOAD & RESPONSE TYPES
// ============================================================================

#[derive(Debug, Clone)]
pub enum RequestPayload {
    GetTemperature(GetTemperatureQuery),
    GetAllSensorData(GetAllSensorDataQuery),
    GetColor(GetColorQuery),
    HealthCheck(HealthCheckQuery),
    SetColor(SetColorCommand),
    SetTemperature(SetTemperatureCommand),
    PrepareConfigUpdate(PrepareConfigUpdateCommand),
    PrepareShutdown(PrepareShutdownCommand),
    CommitConfigUpdate(CommitConfigUpdateCommand),
    RollbackConfigUpdate(RollbackConfigUpdateCommand),
}

#[derive(Debug, Clone)]
pub enum Response {
    Temperature {
        device_id: String,
        value: f32,
    },
    SensorData {
        sensors: HashMap<String, f32>,
    },
    Color {
        device_id: String,
        color: String,
    },
    Health {
        service: ServiceType,
        status: String,
    },
    Success,
    Error(String),
}

// ============================================================================
// CONFIG CHANGE TYPES
// ============================================================================

#[derive(Debug, Clone)]
pub enum ConfigChangeType {
    HotReload,
    ColdRestart { changed_sections: Vec<String> },
}

// ============================================================================
// EVENT TYPES FOR NOTIFICATIONS
// ============================================================================

#[derive(Debug, Clone)]
pub enum Event {
    TemperatureChanged(HashMap<String, f32>),
    ConfigReloaded,
    ConfigChangeDetected(ConfigChangeType),
    CommitConfigUpdate {
        transaction_id: u64,
    },
    RollbackConfigUpdate {
        transaction_id: u64,
    },
    DeviceConnected {
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<String>,
    },
    DeviceDisconnected {
        vendor_id: u16,
        product_id: u16,
        serial_number: Option<String>,
    },
    SystemShutdown,
}

// ============================================================================
// FROM IMPLEMENTATIONS FOR AUTOMATIC CONVERSION
// ============================================================================

impl From<GetTemperatureQuery> for RequestPayload {
    fn from(query: GetTemperatureQuery) -> Self {
        RequestPayload::GetTemperature(query)
    }
}

impl From<GetAllSensorDataQuery> for RequestPayload {
    fn from(query: GetAllSensorDataQuery) -> Self {
        RequestPayload::GetAllSensorData(query)
    }
}

impl From<GetColorQuery> for RequestPayload {
    fn from(query: GetColorQuery) -> Self {
        RequestPayload::GetColor(query)
    }
}

impl From<HealthCheckQuery> for RequestPayload {
    fn from(query: HealthCheckQuery) -> Self {
        RequestPayload::HealthCheck(query)
    }
}

impl From<SetColorCommand> for RequestPayload {
    fn from(command: SetColorCommand) -> Self {
        RequestPayload::SetColor(command)
    }
}

impl From<SetTemperatureCommand> for RequestPayload {
    fn from(command: SetTemperatureCommand) -> Self {
        RequestPayload::SetTemperature(command)
    }
}

impl From<PrepareConfigUpdateCommand> for RequestPayload {
    fn from(command: PrepareConfigUpdateCommand) -> Self {
        RequestPayload::PrepareConfigUpdate(command)
    }
}

impl From<PrepareShutdownCommand> for RequestPayload {
    fn from(command: PrepareShutdownCommand) -> Self {
        RequestPayload::PrepareShutdown(command)
    }
}

impl From<CommitConfigUpdateCommand> for RequestPayload {
    fn from(command: CommitConfigUpdateCommand) -> Self {
        RequestPayload::CommitConfigUpdate(command)
    }
}

impl From<RollbackConfigUpdateCommand> for RequestPayload {
    fn from(command: RollbackConfigUpdateCommand) -> Self {
        RequestPayload::RollbackConfigUpdate(command)
    }
}

// ============================================================================
// EXECUTABLE REQUEST TRAIT
// ============================================================================

pub trait ExecutableRequest {
    type Output;
    fn execute_on(
        self,
        broker: &MessageBroker,
    ) -> impl std::future::Future<Output = Result<Self::Output>> + Send;
}

impl ExecutableRequest for GetTemperatureQuery {
    type Output = Response;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<Response> {
        broker.execute_single::<Self>(self.into()).await
    }
}

impl ExecutableRequest for GetAllSensorDataQuery {
    type Output = Response;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<Response> {
        broker.execute_single::<Self>(self.into()).await
    }
}

impl ExecutableRequest for GetColorQuery {
    type Output = Response;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<Response> {
        broker.execute_single::<Self>(self.into()).await
    }
}

impl ExecutableRequest for SetColorCommand {
    type Output = Response;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<Response> {
        broker.execute_single::<Self>(self.into()).await
    }
}

impl ExecutableRequest for SetTemperatureCommand {
    type Output = Response;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<Response> {
        broker.execute_single::<Self>(self.into()).await
    }
}

impl ExecutableRequest for HealthCheckQuery {
    type Output = HashMap<ServiceType, Response>;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<HashMap<ServiceType, Response>> {
        broker.execute_multiple::<Self>(self.into()).await
    }
}

// Multiple target commands
impl ExecutableRequest for PrepareConfigUpdateCommand {
    type Output = HashMap<ServiceType, Response>;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<HashMap<ServiceType, Response>> {
        broker.execute_multiple::<Self>(self.into()).await
    }
}

impl ExecutableRequest for PrepareShutdownCommand {
    type Output = HashMap<ServiceType, Response>;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<HashMap<ServiceType, Response>> {
        broker.execute_multiple::<Self>(self.into()).await
    }
}

impl ExecutableRequest for CommitConfigUpdateCommand {
    type Output = HashMap<ServiceType, Response>;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<HashMap<ServiceType, Response>> {
        broker.execute_multiple::<Self>(self.into()).await
    }
}

impl ExecutableRequest for RollbackConfigUpdateCommand {
    type Output = HashMap<ServiceType, Response>;

    #[inline]
    async fn execute_on(self, broker: &MessageBroker) -> Result<HashMap<ServiceType, Response>> {
        broker.execute_multiple::<Self>(self.into()).await
    }
}

// ============================================================================
// REQUEST STRUCTURE
// ============================================================================

#[derive(Debug)]
pub struct Request {
    pub payload: Arc<RequestPayload>,
    pub response_channel: oneshot::Sender<Result<Response>>,
}

// ============================================================================
// MESSAGE BROKER IMPLEMENTATION
// ============================================================================

pub struct MessageBroker {
    event_sender: broadcast::Sender<Event>,
    service_handlers: Arc<DashMap<ServiceType, mpsc::Sender<Request>>>,
}

impl MessageBroker {
    pub fn new() -> Self {
        info!("Initializing MessageBroker with default capacity");
        let (event_sender, _) = broadcast::channel(1000);
        Self {
            event_sender,
            service_handlers: Arc::new(DashMap::new()),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        info!("Initializing MessageBroker with capacity: {}", capacity);
        let (event_sender, _) = broadcast::channel(capacity);
        Self {
            event_sender,
            service_handlers: Arc::new(DashMap::new()),
        }
    }

    pub fn register_handler(&self, service_type: ServiceType, handler: mpsc::Sender<Request>) {
        let e = self.service_handlers.insert(service_type, handler);
        if e.is_some() {
            info!(
                "Replaced existing handler for {:?} with new handler",
                service_type
            );
        } else {
            info!("Registered new handler for {:?}", service_type);
        }

        info!(
            "Current handler count: {}",
            self.service_handlers.len()
        );
    }

    pub fn notify(&self, event: Event) -> Result<()> {
        self.event_sender.send(event)?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.event_sender.subscribe()
    }

    #[inline]
    pub async fn execute<T>(&self, request: T) -> Result<T::Output>
    where
        T: ExecutableRequest,
    {
        request.execute_on(self).await
    }

    async fn execute_single<T>(&self, request: RequestPayload) -> Result<Response>
    where
        T: SingleTarget,
    {
        let service_type = T::TARGET;

        let handler = self
            .service_handlers
            .get(&service_type)
            .ok_or_else(|| anyhow::anyhow!("No handler registered for {:?}", service_type))?;

        let (response_tx, response_rx) = oneshot::channel();

        handler
            .send(Request {
                payload: Arc::new(request),
                response_channel: response_tx,
            })
            .await?;

        response_rx.await?
    }

    async fn execute_multiple<T>(
        &self,
        request: RequestPayload,
    ) -> Result<HashMap<ServiceType, Response>>
    where
        T: MultipleTarget,
    {
        let target_services = T::TARGETS;

        let payload_arc = Arc::new(request);

        info!(
            "Executing request on multiple targets: {:?}",
            target_services
        );

        info!("Total registered services: {}", self.service_handlers.len());

        let tasks: Vec<_> = target_services
            .iter()
            .filter_map(|&service_type| {
                info!("Checking for handler for service type: {:?}", service_type);
                let handler = self.service_handlers.get(&service_type);

                if handler.is_none() {
                    info!("No handler found for service type: {:?}", service_type);
                    return None;
                }

                handler.map(|handler| {
                    info!("Found handler for service type: {:?}", service_type);
                    let handler = handler.value().clone();
                    let (response_tx, response_rx) = oneshot::channel();
                    let req = Request {
                        payload: payload_arc.clone(), // Cheap Arc pointer clone (8 bytes)
                        response_channel: response_tx,
                    };

                    (service_type, handler.clone(), req, response_rx)
                })
            })
            .map(|(service_type, handler, req, response_rx)| {
                info!("Spawning task for service type: {:?}", service_type);
                tokio::spawn(async move {
                    let _ = handler.send(req).await;
                    (service_type, response_rx.await)
                })
            })
            .collect();

        let results = futures::future::join_all(tasks).await;
        info!(
            "Received responses for multiple targets: {:?}",
            results.len()
        );
        let mut responses = HashMap::new();

        for result in results.into_iter().flatten() {
            let (service_type, response_result) = result;
            if let Ok(Ok(response)) = response_result {
                responses.insert(service_type, response);
            }
        }

        Ok(responses)
    }

    pub fn handler_count(&self) -> usize {
        self.service_handlers.len()
    }

    pub fn has_handler(&self, service_type: ServiceType) -> bool {
        self.service_handlers.contains_key(&service_type)
    }
}

impl Clone for MessageBroker {
    fn clone(&self) -> Self {
        Self {
            event_sender: self.event_sender.clone(),
            service_handlers: Arc::clone(&self.service_handlers),
        }
    }
}

impl Default for MessageBroker {
    fn default() -> Self {
        Self::new()
    }
}

pub type EventBus = MessageBroker;
