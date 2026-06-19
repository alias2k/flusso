use super::*;

#[tokio::test]
async fn publishes_and_consumes_in_order() {
    let (producer, mut consumer) = channel::<i32>(8);
    producer.publish(1).await.unwrap();
    producer.publish(2).await.unwrap();
    drop(producer); // close the queue so it drains then ends

    let (first, first_handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(first, 1);
    first_handle.ack().await.unwrap();

    let (item, handle) = consumer.recv().await.unwrap().unwrap().into_parts();
    assert_eq!(item, 2);
    handle.ack().await.unwrap();

    assert!(consumer.recv().await.unwrap().is_none());
}

#[tokio::test]
async fn publishing_to_a_closed_queue_errors() {
    let (producer, consumer) = channel::<i32>(1);
    drop(consumer);
    assert!(matches!(producer.publish(1).await, Err(QueueError::Closed)));
}
