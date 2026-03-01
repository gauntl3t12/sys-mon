//! Re-implementation of helloworld / subscriber example in CycloneDDS

use dds::{cmn::tsSystemInfoMsg, consts::SystemInfoMsgTopic};
use futures::{FutureExt, StreamExt};
use rustdds::{
    DataReaderStatus, DomainParticipantBuilder, QosPolicyBuilder, TopicKind, policy::Reliability,
};

fn main() {
    // Set Ctrl-C handler
    let (stop_sender, stop_receiver) = smol::channel::bounded(1);
    ctrlc::set_handler(move || {
        stop_sender.send_blocking(()).unwrap_or(());
        // ignore errors, as we are quitting anyway
    })
    .expect("Error setting Ctrl-C handler");
    println!("Press Ctrl-C to quit.");

    let domain_id = 0;
    let domain_participant = DomainParticipantBuilder::new(domain_id)
        .build()
        .unwrap_or_else(|e| panic!("DomainParticipant construction failed: {e:?}"));

    let qos = QosPolicyBuilder::new()
        .reliability(Reliability::BestEffort)
        .build();

    let topic = domain_participant
        .create_topic(
            // We can internally call the Rust type "tsSystemInfoMsg" whatever we want,
            // but these strings must match whatever our counterparts expect
            // to see over RTPS.
            SystemInfoMsgTopic.to_string(), // topic name
            SystemInfoMsgTopic.to_string(), // type name
            &qos,
            TopicKind::NoKey,
        )
        .unwrap_or_else(|e| panic!("create_topic failed: {e:?}"));

    let subscriber = domain_participant.create_subscriber(&qos).unwrap();
    let data_reader = subscriber
        .create_datareader_no_key_cdr::<tsSystemInfoMsg>(&topic, None) // None = get qos policy from publisher
        .unwrap();

    // set up async executor to run concurrent tasks
    smol::block_on(async {
        let mut sample_stream = data_reader.async_sample_stream();
        let mut event_stream = sample_stream.async_event_stream();

        println!("Waiting for hello messages.");
        loop {
            futures::select! {
              _ = stop_receiver.recv().fuse() =>
                break,

              result = sample_stream.select_next_some() => {
                match result {
                  Ok(s) => {
                    println!("Received: {:?}", s.into_value())
                  }
                  Err(e) =>
                    println!("Oh no, DDS read error: {e:?}"),
                }
              }

              e = event_stream.select_next_some() => {
                match e {
                  DataReaderStatus::SubscriptionMatched{ current,..} => {
                    if current.count_change() > 0 {
                      println!("Currently connected to {} publishers", current.count_change());
                    } else {
                      println!("Publisher disconnected");
                    }
                  }
                  _ =>
                    println!("DataReader event: {e:?}"),
                }
              }
            } // select!
        } // loop
    });
}
