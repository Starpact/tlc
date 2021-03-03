import { useState, useEffect } from "react";
import { Grid } from 'react-virtualized';
import * as tauri from 'tauri/api/tauri';
import 'react-virtualized/styles.css'
import { Box } from "@chakra-ui/react";

function ITable({
  config,
  setErrMsg,
  scrollToColumn,
  setScrollToColumn,
  scrollToRow,
  setScrollToRow,
}) {
  const [daq, setDaq] = useState(null);

  useEffect(() => {
    tauri.promisified({ cmd: "getDaq" })
      .then(ok => setDaq(ok))
      .catch(err => setErrMsg(err));
  }, [config]);

  function cellRenderer({ columnIndex, key, rowIndex, style }) {
    style = JSON.parse(JSON.stringify(style));
    style.border = "1px solid #98971a";
    if (columnIndex === scrollToColumn && rowIndex === scrollToRow) {
      style.backgroundColor = "#cc241d";
      style.color = "#282828";
    } else if (columnIndex === scrollToColumn || rowIndex === scrollToRow) {
      style.backgroundColor = "#d79921";
      style.color = "#282828";
    }

    return (
      <div
        key={key}
        style={style}
        onClick={() => {
          setScrollToColumn(columnIndex);
          setScrollToRow(rowIndex);
        }}
      >
        {daq.data[rowIndex * daq.dim[1] + columnIndex].toFixed(2)}
      </div>
    );
  }

  return (
    <Box
      color="#fbf1c7"
      textAlign="center"
      width={900}
      height={300}
    >
      {!!daq &&
        <Grid
          width={900}
          height={300}
          cellRenderer={cellRenderer}
          columnCount={daq.dim[1]}
          columnWidth={100}
          rowCount={daq.dim[0]}
          rowHeight={30}
        />}
    </Box >
  )
}

export default ITable