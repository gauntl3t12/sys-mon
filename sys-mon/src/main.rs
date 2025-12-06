use std::{
    time::Duration,
};

use rustdds::{
    DomainParticipantBuilder, QosPolicyBuilder, TopicKind, DataWriterStatus, StatusEvented,
    policy::Reliability,
    no_key::DataWriter,
};
use smol::Timer;
use futures::{FutureExt, StreamExt, TryFutureExt};

use dds::{
    cmn::tsSystemInfoMsg,
    // consts::SystemInfoMsgTopic
};
use monitor;
use sysinfo::{Components, Disks, System};

pub const SystemInfoMsgTopic: &str = "SystemInfoMsg";

fn main() {
        let domain_id = 0;
    let domain_participant = DomainParticipantBuilder::new(domain_id)
        .build()
        .unwrap_or_else(|e| panic!("DomainParticipant construction failed: {e:?}"));

    let qos = QosPolicyBuilder::new()
        .reliability(Reliability::Reliable {
            max_blocking_time: rustdds::Duration::from_secs(1),
        })
        .build();

    let topic = domain_participant
        .create_topic(
            SystemInfoMsgTopic.to_string(), // topic name
            SystemInfoMsgTopic.to_string(), // type name
            &qos,
            TopicKind::NoKey,
        )
        .unwrap_or_else(|e| panic!("create_topic failed: {e:?}"));

    let publisher = domain_participant.create_publisher(&qos).unwrap();
    let dds_writer =publisher
        .create_datawriter_no_key_cdr::<tsSystemInfoMsg>(&topic, None) // None = get qos policy from publisher
        .unwrap();
    // let dds_writer = prep_dds_writer();
    
    let mut info = monitor::SystemStructs::new(
        System::new_all(),
        Components::new_with_refreshed_list(),
        Disks::new_with_refreshed_list(),
    );

    smol::block_on(async {
        let mut datawriter_event_stream = dds_writer.as_async_status_stream();
        let (write_trigger_sender, write_trigger_receiver) = smol::channel::bounded(1);

        loop {
            let sys_info = monitor::gather_system_info(&mut info);
            futures::select! {
              _ = write_trigger_receiver.recv().fuse() => {
                println!("Sending status");
                dds_writer.async_write(sys_info, None)
                  .unwrap_or_else(|e| log::error!("DataWriter async_write failed: {e:?}"))
                  .await;
                Timer::after(Duration::from_secs(1)).await;
                write_trigger_sender.send(()).await.unwrap();
              }
              e = datawriter_event_stream.select_next_some() => {
                match e {
                  // If we get a matching subscription, trigger the send
                  DataWriterStatus::PublicationMatched{..} => {
                    println!("Matched with subscriber");
                    // Wait for a while so that subscriber also recognizes us.
                    // There is no two- or three-way handshake in pub/sub matching,
                    // so we cannot know if the other side is immediately ready.
                    Timer::after(Duration::from_secs(1)).await;
                    write_trigger_sender.send(()).await.unwrap();
                  }
                  _ =>
                    println!("DataWriter event: {e:?}"),
                }
              }
            } // select!
        } // loop
    });
}
