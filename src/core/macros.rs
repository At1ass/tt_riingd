/// Realize:
///   * private::Sealed
///   * SingleTarget
///   * From<T> for RequestPayload
///   * ExecutableRequest (вывод ― `Response`)
#[macro_export]
macro_rules! impl_single_target_request {
    ($ty:ty, $payload_variant:ident, $service:path) => {
        impl private::Sealed for $ty {}

        impl SingleTarget for $ty {
            const TARGET: ServiceType = $service;
        }

        impl From<$ty> for RequestPayload {
            #[inline]
            fn from(v: $ty) -> Self {
                RequestPayload::$payload_variant(v)
            }
        }

        impl ExecutableRequest for $ty {
            type Output = Response;

            #[inline]
            async fn execute_on(self, broker: &MessageBroker) -> anyhow::Result<Self::Output> {
                broker.execute_single::<Self>(self.into()).await
            }
        }
    };
}

/// Аналог для «многоточечной» команды/запроса (вывод ― `HashMap<ServiceType, Response>`).
#[macro_export]
macro_rules! impl_multiple_target_request {
    ($ty:ty, $payload_variant:ident, [$($service:path),+ $(,)?]) => {
        impl private::Sealed for $ty {}

        impl MultipleTarget for $ty {
            const TARGETS: &'static [ServiceType] = &[$($service),+];
        }

        impl From<$ty> for RequestPayload {
            #[inline]
            fn from(v: $ty) -> Self {
                RequestPayload::$payload_variant(v)
            }
        }

        impl ExecutableRequest for $ty {
            type Output = std::collections::HashMap<ServiceType, Response>;

            #[inline]
            async fn execute_on(
                self,
                broker: &MessageBroker,
            ) -> anyhow::Result<Self::Output> {
                broker.execute_multiple::<Self>(self.into()).await
            }
        }
    };
}
